"use client";

import Link from "next/link";
import { useEffect, useId, useRef, useState } from "react";

export type NavConsoleDropdownCopy = {
  adminPanel: string;
  consoles: string;
  creatorConsole: string;
};

type NavConsoleDropdownProps = {
  copy: NavConsoleDropdownCopy;
};

/** Renders the observer console menu and closes it on outside interaction. */
export function NavConsoleDropdown({ copy }: NavConsoleDropdownProps) {
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

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

  return (
    <div
      className="nav-dropdown"
      data-open={open ? "true" : undefined}
      ref={rootRef}
    >
      <button
        aria-controls={menuId}
        aria-expanded={open}
        aria-haspopup="menu"
        className="nav-dropdown-trigger"
        onClick={() => setOpen((current) => !current)}
        ref={triggerRef}
        type="button"
      >
        {copy.consoles}
      </button>
      {open ? (
        <div className="nav-dropdown-menu" id={menuId} role="menu">
          <Link
            className="nav-dropdown-item"
            href="/creator"
            onClick={() => setOpen(false)}
            rel="noopener noreferrer"
            role="menuitem"
            target="_blank"
          >
            {copy.creatorConsole}
          </Link>
          <Link
            className="nav-dropdown-item"
            href="/admin"
            onClick={() => setOpen(false)}
            rel="noopener noreferrer"
            role="menuitem"
            target="_blank"
          >
            {copy.adminPanel}
          </Link>
        </div>
      ) : null}
    </div>
  );
}
