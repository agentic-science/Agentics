"use client";

import { useEffect } from "react";

/** Resets the browser viewport when a route surface should open from the top. */
export function ScrollToTop() {
  useEffect(() => {
    window.scrollTo({ top: 0, left: 0, behavior: "instant" });
  }, []);

  return null;
}
