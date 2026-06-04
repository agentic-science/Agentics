"use client";

import { ShieldCheck } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import { useEffect, useMemo, useState } from "react";
import { AdminIdentityPanel } from "@/components/admin/AdminIdentityPanel";
import {
  type AdminRefresh,
  CapacityPanel,
  ChallengeAdminPanel,
  OperationsPanel,
  OverviewPanel,
} from "@/components/admin/AdminPanels";
import { ChallengeReviewRecordPanel } from "@/components/admin/ChallengeReviewRecordPanel";
import { CredentialPanel } from "@/components/admin/CredentialPanel";
import { adminErrorMessage } from "@/components/admin/errors";
import { PioneerCodePanel } from "@/components/admin/PioneerCodePanel";
import { AdminApiError } from "@/lib/adminApi";
import {
  clearAdminDashboard,
  mutateAdminDashboard,
  useAdminDashboard,
  useAdminSession,
} from "@/lib/adminData";
import { logoutHuman, startGithubLogin } from "@/lib/authApi";

/** Describes the admin tab shape used by this module. */
type AdminTab =
  | "overview"
  | "challenges"
  | "reviewRecords"
  | "pioneer-codes"
  | "identity"
  | "capacity"
  | "operations";

/** Renders the admin console component. */
export function AdminConsole() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [csrfToken, setCsrfToken] = useState("");
  const [activeTab, setActiveTab] = useState<AdminTab>("overview");
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const restoredSession = useAdminSession();
  const hasAdminRole =
    restoredSession.session?.roles.includes("admin") ?? false;
  const dashboard = useAdminDashboard(hasAdminRole ? csrfToken : "");
  const data = dashboard.data;

  const loginAndRefresh: AdminRefresh = async (options = {}) => {
    setLoading(true);
    setError(null);
    try {
      const response = await startGithubLogin("", "/admin");
      window.location.href = response.authorization_url;
      if (!options.quiet) {
        setMessage(t("messages.redirectingToGithub"));
      }
    } catch (e) {
      setError(
        adminErrorMessage(e, {
          accessDenied: t("messages.accessDenied"),
          unknown: t("messages.unknown"),
        }),
      );
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (
      restoredSession.session &&
      restoredSession.session.csrf_token !== csrfToken
    ) {
      setCsrfToken(restoredSession.session.csrf_token);
    } else if (!restoredSession.session && csrfToken) {
      setCsrfToken("");
    }
  }, [csrfToken, restoredSession.session]);

  useEffect(() => {
    const sessionError = restoredSession.error;
    if (
      sessionError instanceof AdminApiError &&
      sessionError.status !== 401 &&
      sessionError.status !== 403
    ) {
      setError(
        adminErrorMessage(sessionError, {
          accessDenied: t("messages.accessDenied"),
          unknown: t("messages.unknown"),
        }),
      );
    }
  }, [restoredSession.error, t]);

  useEffect(() => {
    if (dashboard.error) {
      setError(
        adminErrorMessage(dashboard.error, {
          accessDenied: t("messages.accessDenied"),
          unknown: t("messages.unknown"),
        }),
      );
    }
  }, [dashboard.error, t]);

  /** Handles sign out for the current session. */
  const signOut = async () => {
    if (!csrfToken) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await logoutHuman(csrfToken);
      await clearAdminDashboard(csrfToken);
      await restoredSession.mutate(undefined, { revalidate: false });
      setCsrfToken("");
      setMessage(t("messages.sessionEnded"));
    } catch (e) {
      setError(
        adminErrorMessage(e, {
          accessDenied: t("messages.accessDenied"),
          unknown: t("messages.unknown"),
        }),
      );
    } finally {
      setLoading(false);
    }
  };

  const refresh: AdminRefresh = async (options = {}) => {
    if (!csrfToken) {
      setError(t("messages.signInBeforeRefresh"));
      return;
    }
    if (!hasAdminRole) {
      setError(t("messages.accessDenied"));
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await mutateAdminDashboard(csrfToken);
      if (!options.quiet) {
        setMessage(t("messages.dataRefreshed"));
      }
    } catch (e) {
      setError(
        adminErrorMessage(e, {
          accessDenied: t("messages.accessDenied"),
          unknown: t("messages.unknown"),
        }),
      );
    } finally {
      setLoading(false);
    }
  };

  /** Handles status counts behavior for this component. */
  const statusCounts = useMemo(() => {
    return data.submissions.items.reduce<Record<string, number>>(
      (acc, item) => {
        acc[item.status] = (acc[item.status] ?? 0) + 1;
        return acc;
      },
      {},
    );
  }, [data.submissions.items]);

  return (
    <div className="flex flex-col gap-6">
      <section className="card-elevated">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-6">
          <div>
            <span className="badge badge-official mb-4">
              <ShieldCheck className="w-3 h-3" />
              {t("hero.badge")}
            </span>
            <h1
              className="text-h1 font-bold leading-h1"
              style={{ fontFamily: "var(--font-sans)" }}
            >
              {t("hero.title")}
            </h1>
            <p className="mt-3 max-w-2xl text-body leading-body text-fg-secondary">
              {t("hero.description")}
            </p>
          </div>
          <CredentialPanel
            sessionLogin={restoredSession.session?.github_login ?? null}
            hasAdminRole={hasAdminRole}
            onLogin={loginAndRefresh}
            onLogout={signOut}
            loading={loading || restoredSession.isLoading}
          />
        </div>
      </section>

      {error ? (
        <div className="card border-danger/40 text-danger">{error}</div>
      ) : null}
      {message ? (
        <div className="card border-success/30 text-success">{message}</div>
      ) : null}
      {restoredSession.session && !hasAdminRole ? (
        <div className="card border-warning/40 text-warning">
          {t("messages.accessDenied")}
        </div>
      ) : null}

      {hasAdminRole ? (
        <nav className="tab-list flex-wrap">
          {[
            ["overview", t("tabs.overview")],
            ["challenges", t("tabs.challenges")],
            ["reviewRecords", t("tabs.reviewRecords")],
            ["pioneer-codes", t("tabs.pioneerCodes")],
            ["identity", t("tabs.identity")],
            ["capacity", t("tabs.capacity")],
            ["operations", t("tabs.operations")],
          ].map(([id, label]) => (
            <button
              key={id}
              type="button"
              className={`tab-link ${activeTab === id ? "active" : ""}`}
              onClick={() => setActiveTab(id as AdminTab)}
            >
              {label}
            </button>
          ))}
        </nav>
      ) : null}

      {hasAdminRole && activeTab === "overview" ? (
        <OverviewPanel data={data} statusCounts={statusCounts} />
      ) : null}
      {hasAdminRole && activeTab === "challenges" ? (
        <ChallengeAdminPanel
          challenges={data.challenges.items}
          locale={locale}
        />
      ) : null}
      {hasAdminRole && activeTab === "reviewRecords" ? (
        <ChallengeReviewRecordPanel
          csrfToken={csrfToken}
          reviewRecords={data.reviewRecords.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {hasAdminRole && activeTab === "pioneer-codes" ? (
        <PioneerCodePanel
          csrfToken={csrfToken}
          pioneerCodes={data.pioneerCodes}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {hasAdminRole && activeTab === "identity" ? (
        <AdminIdentityPanel
          csrfToken={csrfToken}
          currentHumanId={restoredSession.session?.human_id ?? null}
          humans={data.humans}
          serviceTokens={data.adminServiceTokens}
          locale={locale}
          onRefresh={refresh}
          onSessionChanged={async () => {
            await restoredSession.mutate();
          }}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {hasAdminRole && activeTab === "capacity" ? (
        <CapacityPanel capacity={data.capacity} />
      ) : null}
      {hasAdminRole && activeTab === "operations" ? (
        <OperationsPanel
          csrfToken={csrfToken}
          submissions={data.submissions.items}
          heartbeats={data.heartbeats.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
    </div>
  );
}
