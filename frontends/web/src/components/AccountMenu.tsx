"use client";

import { UserRound } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import { useEffect, useId, useRef, useState } from "react";
import useSWR, { mutate } from "swr";
import {
  getHumanSession,
  HUMAN_SESSION_CACHE_KEY,
  logoutHuman,
} from "@/lib/authApi";
import { clearCreatorApiTokenCaches } from "@/lib/creatorData";
import type { HumanSessionResponse } from "@/lib/schemas";

/** Renders the shared account controls in the site header. */
export function AccountMenu() {
  const t = useTranslations("account");
  const pathname = usePathname();
  const [returnTo, setReturnTo] = useState(pathname);
  const [open, setOpen] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const menuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const { data: session } = useSWR<HumanSessionResponse>(
    HUMAN_SESSION_CACHE_KEY,
    getHumanSession,
    { shouldRetryOnError: false },
  );
  useEffect(() => {
    setReturnTo(`${pathname}${window.location.search}`);
  }, [pathname]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: PointerEvent) {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (!rootRef.current?.contains(target)) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const signOut = async () => {
    if (!session?.csrf_token) {
      return;
    }
    setSigningOut(true);
    try {
      await logoutHuman(session.csrf_token);
      await mutate(HUMAN_SESSION_CACHE_KEY, undefined, { revalidate: false });
      await clearCreatorApiTokenCaches();
      setOpen(false);
    } finally {
      setSigningOut(false);
    }
  };

  if (!session) {
    return (
      <Link
        className="btn btn-ghost btn-sm"
        href={`/sign-in?return_to=${encodeURIComponent(returnTo)}`}
      >
        <UserRound className="w-4 h-4" />
        {t("signIn")}
      </Link>
    );
  }

  const active = session.status === "active";
  const admin = active && session.roles.includes("admin");
  const creator =
    active &&
    (session.roles.includes("creator") || session.roles.includes("admin"));
  const setupRequired = session.status === "setup_required";

  return (
    <div className="flex items-center gap-2">
      <div
        className="nav-dropdown"
        data-open={open ? "true" : undefined}
        ref={rootRef}
      >
        <button
          aria-controls={menuId}
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label={t("account")}
          className="btn btn-ghost btn-sm px-2"
          onClick={() => setOpen((current) => !current)}
          ref={triggerRef}
          title={`@${session.github_login}`}
          type="button"
        >
          <UserRound className="w-4 h-4" />
        </button>
        {open ? (
          <div
            className="nav-dropdown-menu nav-dropdown-menu-right"
            id={menuId}
            role="menu"
          >
            <div className="px-3 py-2 text-caption text-fg-muted">
              @{session.github_login}
            </div>
            <Link
              className="nav-dropdown-item"
              href="/account"
              onClick={() => setOpen(false)}
              role="menuitem"
            >
              {t("settings")}
            </Link>
            {setupRequired ? (
              <Link
                className="nav-dropdown-item"
                href={`/account/setup?return_to=${encodeURIComponent("/creator")}`}
                onClick={() => setOpen(false)}
                role="menuitem"
              >
                {t("finishSetup")}
              </Link>
            ) : null}
            {creator ? (
              <Link
                className="nav-dropdown-item"
                href="/creator"
                onClick={() => setOpen(false)}
                role="menuitem"
              >
                {t("creatorConsole")}
              </Link>
            ) : null}
            {admin ? (
              <Link
                className="nav-dropdown-item"
                href="/admin"
                onClick={() => setOpen(false)}
                role="menuitem"
              >
                {t("adminPanel")}
              </Link>
            ) : null}
          </div>
        ) : null}
      </div>
      <button
        className="btn btn-ghost btn-sm"
        disabled={signingOut}
        onClick={() => void signOut()}
        type="button"
      >
        {t("signOut")}
      </button>
    </div>
  );
}
