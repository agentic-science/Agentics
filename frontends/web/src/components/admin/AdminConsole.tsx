"use client";

import { ShieldCheck } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import { useEffect, useMemo, useState } from "react";
import {
  type AdminRefresh,
  CapacityPanel,
  ChallengeAdminPanel,
  OperationsPanel,
  OverviewPanel,
} from "@/components/admin/AdminPanels";
import { ChallengeDraftReviewPanel } from "@/components/admin/ChallengeDraftReviewPanel";
import { CredentialPanel } from "@/components/admin/CredentialPanel";
import { adminErrorMessage } from "@/components/admin/errors";
import { PioneerCodePanel } from "@/components/admin/PioneerCodePanel";
import {
  AdminApiError,
  type AdminCredentials,
  adminLogin,
  adminLogout,
} from "@/lib/adminApi";
import {
  clearAdminDashboard,
  mutateAdminDashboard,
  useAdminDashboard,
  useAdminSession,
} from "@/lib/adminData";

/** Describes the admin tab shape used by this module. */
type AdminTab =
  | "overview"
  | "challenges"
  | "drafts"
  | "pioneer-codes"
  | "capacity"
  | "operations";

/** Renders the admin console component. */
export function AdminConsole() {
  const locale = useLocale();
  const t = useTranslations("admin");
  const [credentials, setCredentials] = useState<AdminCredentials>({
    username: "admin",
    password: "",
  });
  const [csrfToken, setCsrfToken] = useState("");
  const [sessionUsername, setSessionUsername] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<AdminTab>("overview");
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const restoredSession = useAdminSession();
  const dashboard = useAdminDashboard(csrfToken);
  const data = dashboard.data;

  const isConfigured = credentials.username.trim() && credentials.password;

  const loginAndRefresh: AdminRefresh = async (options = {}) => {
    if (!isConfigured) {
      setError(t("messages.enterCredentials"));
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const session = await adminLogin(credentials);
      setCsrfToken(session.csrf_token);
      setSessionUsername(session.username);
      setCredentials({ username: session.username, password: "" });
      await mutateAdminDashboard(session.csrf_token);
      if (!options.quiet) {
        setMessage(t("messages.sessionStarted"));
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
    if (restoredSession.session && !csrfToken) {
      setCsrfToken(restoredSession.session.csrf_token);
      setSessionUsername(restoredSession.session.username);
      setCredentials({
        username: restoredSession.session.username,
        password: "",
      });
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
      setSessionUsername(null);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await adminLogout(csrfToken);
      await clearAdminDashboard(csrfToken);
      setCsrfToken("");
      setSessionUsername(null);
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
            credentials={credentials}
            sessionUsername={sessionUsername}
            onChange={setCredentials}
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

      <nav className="tab-list overflow-x-auto">
        {[
          ["overview", t("tabs.overview")],
          ["challenges", t("tabs.challenges")],
          ["drafts", t("tabs.drafts")],
          ["pioneer-codes", t("tabs.pioneerCodes")],
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

      {activeTab === "overview" ? (
        <OverviewPanel data={data} statusCounts={statusCounts} />
      ) : null}
      {activeTab === "challenges" ? (
        <ChallengeAdminPanel
          challenges={data.challenges.items}
          locale={locale}
        />
      ) : null}
      {activeTab === "drafts" ? (
        <ChallengeDraftReviewPanel
          csrfToken={csrfToken}
          drafts={data.drafts.items}
          locale={locale}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {activeTab === "pioneer-codes" ? (
        <PioneerCodePanel
          csrfToken={csrfToken}
          pioneerCodes={data.pioneerCodes}
          onRefresh={refresh}
          onError={setError}
          onMessage={setMessage}
        />
      ) : null}
      {activeTab === "capacity" ? (
        <CapacityPanel capacity={data.capacity} />
      ) : null}
      {activeTab === "operations" ? (
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
