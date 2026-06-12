//! Production Compose bridge egress checks.
//!
//! Docker usually owns bridge forwarding rules, but host firewall state can
//! drift after repeated production rehearsals. These checks keep the Compose
//! service bridge able to reach the host's default outbound interface without
//! opening broad bridge-to-bridge forwarding.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::process::Stdio;
use std::time::Duration;

#[cfg(unix)]
use nix::unistd::Uid;
use serde::Deserialize;
use tokio::process::Command;

use crate::support::{DEFAULT_OUTPUT_LIMIT_BYTES, ReportLine, run_command};

use super::{ComposeContext, ComposeProdError, docker_output};

const GITHUB_EGRESS_HOST: &str = "github.com";
const GITHUB_EGRESS_PORT: u16 = 443;

pub(super) async fn ensure_compose_bridge_egress(
    context: &ComposeContext,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    let network_name = context.default_network_name();
    let bridge_name = compose_network_bridge_name(context, &network_name).await?;
    ensure_bridge_egress(
        context,
        "compose bridge egress",
        &format!("{network_name} ({bridge_name})"),
        &bridge_name,
    )
    .await
}

pub(super) async fn ensure_bridge_egress(
    context: &ComposeContext,
    report_name: &'static str,
    bridge_label: &str,
    bridge_name: &str,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    if !cfg!(target_os = "linux") {
        return Ok(vec![ReportLine::skip(
            report_name,
            "iptables bridge forwarding guard is Linux-only",
        )]);
    }

    let outbound_interface = host_default_route_interface(context).await?;
    let rule_specs = bridge_egress_rules(bridge_name, &outbound_interface);
    let mut inserted_rules = Vec::new();
    for rule in &rule_specs {
        if ensure_iptables_rule(context, rule).await? == RuleState::Inserted {
            inserted_rules.push(rule.name);
        }
    }
    let action = if inserted_rules.is_empty() {
        "verified"
    } else {
        "installed"
    };
    Ok(vec![ReportLine::pass(
        report_name,
        format!(
            "{action} DOCKER-USER forwarding between {bridge_label} and default interface {outbound_interface}"
        ),
    )])
}

pub(super) async fn check_api_github_egress(
    context: &ComposeContext,
) -> Result<Vec<ReportLine>, ComposeProdError> {
    let api_container = compose_service_container_id(context, "api").await?;
    let mut args = vec![
        OsString::from("exec"),
        OsString::from(api_container),
        OsString::from("python3"),
        OsString::from("-c"),
    ];
    args.push(OsString::from(format!(
        r#"import socket, ssl
raw = socket.create_connection(("{GITHUB_EGRESS_HOST}", {GITHUB_EGRESS_PORT}), timeout=10)
with ssl.create_default_context().wrap_socket(raw, server_hostname="{GITHUB_EGRESS_HOST}") as sock:
    sock.version()
print("ok")
"#
    )));
    let output = docker_output(context, args, Duration::from_secs(20)).await?;
    if output.success() {
        return Ok(vec![ReportLine::pass(
            "api github egress",
            format!("API container can open TLS to {GITHUB_EGRESS_HOST}:{GITHUB_EGRESS_PORT}"),
        )]);
    }
    Ok(vec![ReportLine::fail(
        "api github egress",
        format!(
            "API container cannot open TLS to {GITHUB_EGRESS_HOST}:{GITHUB_EGRESS_PORT}: {}",
            output.combined()
        ),
    )])
}

async fn compose_network_bridge_name(
    context: &ComposeContext,
    network_name: &str,
) -> Result<String, ComposeProdError> {
    let output = docker_output(
        context,
        ["network", "inspect", network_name],
        Duration::from_secs(30),
    )
    .await?;
    if !output.success() {
        return Err(ComposeProdError::Process(format!(
            "failed to inspect Compose network `{network_name}`: {}",
            output.combined()
        )));
    }
    let mut networks = serde_json::from_str::<Vec<DockerNetworkInspect>>(&output.stdout)
        .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))?;
    let network = networks.pop().ok_or_else(|| {
        ComposeProdError::InvalidConfig(format!(
            "docker network inspect returned no network for `{network_name}`"
        ))
    })?;
    bridge_name_from_network(&network)
}

fn bridge_name_from_network(network: &DockerNetworkInspect) -> Result<String, ComposeProdError> {
    if let Some(name) = network
        .options
        .get("com.docker.network.bridge.name")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(name.to_string());
    }
    let id = network.id.trim();
    let prefix = id.get(..12).ok_or_else(|| {
        ComposeProdError::InvalidConfig(format!(
            "Docker network id `{id}` is too short to derive a bridge name"
        ))
    })?;
    Ok(format!("br-{prefix}"))
}

async fn host_default_route_interface(
    context: &ComposeContext,
) -> Result<String, ComposeProdError> {
    let output = host_output(
        context,
        "ip",
        ["-json", "route", "show", "default"],
        Duration::from_secs(10),
    )
    .await?;
    if !output.success() {
        return Err(ComposeProdError::Process(format!(
            "failed to inspect the host default route: {}",
            output.combined()
        )));
    }
    let routes = serde_json::from_str::<Vec<IpRoute>>(&output.stdout)
        .map_err(|error| ComposeProdError::InvalidConfig(error.to_string()))?;
    select_default_route_interface(&routes).ok_or_else(|| {
        ComposeProdError::InvalidConfig(
            "host default route has no outbound interface; cannot install Compose egress guard"
                .to_string(),
        )
    })
}

fn select_default_route_interface(routes: &[IpRoute]) -> Option<String> {
    routes
        .iter()
        .filter(|route| route.dst.as_deref() == Some("default"))
        .min_by_key(|route| route.metric.unwrap_or(u64::MAX))
        .and_then(|route| route.dev.as_deref())
        .map(str::trim)
        .filter(|dev| !dev.is_empty())
        .map(str::to_string)
}

fn bridge_egress_rules(bridge_name: &str, outbound_interface: &str) -> Vec<IptablesRule> {
    vec![
        IptablesRule {
            name: "bridge outbound",
            check_args: vec![
                "-C".into(),
                "DOCKER-USER".into(),
                "-i".into(),
                bridge_name.into(),
                "-o".into(),
                outbound_interface.into(),
                "-j".into(),
                "ACCEPT".into(),
            ],
            insert_args: vec![
                "-I".into(),
                "DOCKER-USER".into(),
                "1".into(),
                "-i".into(),
                bridge_name.into(),
                "-o".into(),
                outbound_interface.into(),
                "-j".into(),
                "ACCEPT".into(),
            ],
        },
        IptablesRule {
            name: "bridge established return",
            check_args: vec![
                "-C".into(),
                "DOCKER-USER".into(),
                "-i".into(),
                outbound_interface.into(),
                "-o".into(),
                bridge_name.into(),
                "-m".into(),
                "conntrack".into(),
                "--ctstate".into(),
                "RELATED,ESTABLISHED".into(),
                "-j".into(),
                "ACCEPT".into(),
            ],
            insert_args: vec![
                "-I".into(),
                "DOCKER-USER".into(),
                "1".into(),
                "-i".into(),
                outbound_interface.into(),
                "-o".into(),
                bridge_name.into(),
                "-m".into(),
                "conntrack".into(),
                "--ctstate".into(),
                "RELATED,ESTABLISHED".into(),
                "-j".into(),
                "ACCEPT".into(),
            ],
        },
    ]
}

async fn ensure_iptables_rule(
    context: &ComposeContext,
    rule: &IptablesRule,
) -> Result<RuleState, ComposeProdError> {
    let check = iptables_output(context, rule.check_args.iter(), Duration::from_secs(10)).await?;
    if check.success() {
        return Ok(RuleState::Present);
    }

    let insert = iptables_output(context, rule.insert_args.iter(), Duration::from_secs(10)).await?;
    if insert.success() {
        return Ok(RuleState::Inserted);
    }
    Err(ComposeProdError::Process(format!(
        "failed to install iptables rule `{}` with `{}`: {}; \
         containers need this DOCKER-USER rule for egress",
        rule.name,
        iptables_command_text(&rule.insert_args),
        insert.combined()
    )))
}

async fn compose_service_container_id(
    context: &ComposeContext,
    service: &str,
) -> Result<String, ComposeProdError> {
    let output = context.compose_output(["ps", "-q", service]).await?;
    output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            ComposeProdError::Process(format!(
                "Compose service `{service}` has no running container"
            ))
        })
}

async fn host_output<I, S>(
    context: &ComposeContext,
    program: &str,
    args: I,
    timeout: Duration,
) -> Result<crate::support::CommandOutput, ComposeProdError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&context.repo_root)
        .stdin(Stdio::null());
    run_command(command, program, Some(timeout), DEFAULT_OUTPUT_LIMIT_BYTES)
        .await
        .map_err(ComposeProdError::from)
}

async fn iptables_output<'a, I>(
    context: &ComposeContext,
    args: I,
    timeout: Duration,
) -> Result<crate::support::CommandOutput, ComposeProdError>
where
    I: IntoIterator<Item = &'a OsString>,
{
    let mut command = iptables_command();
    command
        .args(args)
        .current_dir(&context.repo_root)
        .stdin(Stdio::null());
    run_command(
        command,
        "iptables",
        Some(timeout),
        DEFAULT_OUTPUT_LIMIT_BYTES,
    )
    .await
    .map_err(ComposeProdError::from)
}

fn iptables_command() -> Command {
    if effective_uid_is_root() {
        return Command::new("iptables");
    }
    let mut command = Command::new("sudo");
    command.arg("-n").arg("iptables");
    command
}

fn effective_uid_is_root() -> bool {
    #[cfg(unix)]
    {
        Uid::effective().is_root()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn iptables_command_text(args: &[OsString]) -> String {
    let mut parts = if effective_uid_is_root() {
        vec!["iptables".to_string()]
    } else {
        vec!["sudo".to_string(), "-n".to_string(), "iptables".to_string()]
    };
    parts.extend(args.iter().map(|arg| arg.to_string_lossy().into_owned()));
    parts.join(" ")
}

#[derive(Debug, Clone, Deserialize)]
struct DockerNetworkInspect {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Options", default)]
    options: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct IpRoute {
    dst: Option<String>,
    dev: Option<String>,
    metric: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IptablesRule {
    name: &'static str,
    check_args: Vec<OsString>,
    insert_args: Vec<OsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleState {
    Present,
    Inserted,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use super::{
        DockerNetworkInspect, IpRoute, bridge_egress_rules, bridge_name_from_network,
        select_default_route_interface,
    };

    /// Verifies Compose bridge detection prefers Docker's explicit bridge option.
    #[test]
    fn bridge_name_uses_docker_bridge_option_when_present() {
        let mut options = HashMap::new();
        options.insert(
            "com.docker.network.bridge.name".to_string(),
            "br-custom-prod".to_string(),
        );
        let bridge = bridge_name_from_network(&DockerNetworkInspect {
            id: "0123456789abcdef".to_string(),
            options,
        })
        .expect("bridge name");
        assert_eq!(bridge, "br-custom-prod");
    }

    /// Verifies Compose bridge detection matches Docker's default br-<network-id> naming.
    #[test]
    fn bridge_name_falls_back_to_network_id_prefix() {
        let bridge = bridge_name_from_network(&DockerNetworkInspect {
            id: "abcdef0123456789abcdef0123456789".to_string(),
            options: HashMap::new(),
        })
        .expect("bridge name");
        assert_eq!(bridge, "br-abcdef012345");
    }

    /// Verifies the host default route selector chooses the lowest metric route.
    #[test]
    fn default_route_interface_uses_lowest_metric() {
        let routes = vec![
            IpRoute {
                dst: Some("default".to_string()),
                dev: Some("slow0".to_string()),
                metric: Some(600),
            },
            IpRoute {
                dst: Some("default".to_string()),
                dev: Some("fast0".to_string()),
                metric: Some(100),
            },
            IpRoute {
                dst: Some("10.0.0.0/8".to_string()),
                dev: Some("internal0".to_string()),
                metric: Some(0),
            },
        ];
        assert_eq!(
            select_default_route_interface(&routes).as_deref(),
            Some("fast0")
        );
    }

    /// Verifies the egress guard allows only the selected bridge and default interface.
    #[test]
    fn bridge_egress_rules_are_scoped_to_default_interface() {
        let rules = bridge_egress_rules("br-prod", "en0");
        let rendered = rules
            .iter()
            .map(|rule| {
                (
                    rule.name,
                    strings(&rule.check_args),
                    strings(&rule.insert_args),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            vec![
                (
                    "bridge outbound",
                    vec![
                        "-C",
                        "DOCKER-USER",
                        "-i",
                        "br-prod",
                        "-o",
                        "en0",
                        "-j",
                        "ACCEPT"
                    ],
                    vec![
                        "-I",
                        "DOCKER-USER",
                        "1",
                        "-i",
                        "br-prod",
                        "-o",
                        "en0",
                        "-j",
                        "ACCEPT"
                    ],
                ),
                (
                    "bridge established return",
                    vec![
                        "-C",
                        "DOCKER-USER",
                        "-i",
                        "en0",
                        "-o",
                        "br-prod",
                        "-m",
                        "conntrack",
                        "--ctstate",
                        "RELATED,ESTABLISHED",
                        "-j",
                        "ACCEPT"
                    ],
                    vec![
                        "-I",
                        "DOCKER-USER",
                        "1",
                        "-i",
                        "en0",
                        "-o",
                        "br-prod",
                        "-m",
                        "conntrack",
                        "--ctstate",
                        "RELATED,ESTABLISHED",
                        "-j",
                        "ACCEPT"
                    ],
                ),
            ]
        );
    }

    fn strings(args: &[OsString]) -> Vec<&str> {
        args.iter()
            .map(|arg| arg.to_str().expect("test args are utf-8"))
            .collect()
    }
}
