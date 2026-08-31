import { useEffect, useState } from "react";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Whether the app is currently drawing dark.
 *
 * Two provider accents resolve per appearance, so a component that guesses
 * gets them wrong half the time — `ProviderIcon` defaulted to `dark = true`
 * and no caller ever passed otherwise, which left Grok's mark on its dark
 * accent in a light window.
 */
export function useDarkMode(): boolean {
  const [dark, setDark] = useState(() => matches());
  useEffect(() => {
    const query = window.matchMedia?.(DARK_QUERY);
    if (!query) return;
    const update = () => setDark(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return dark;
}

function matches(): boolean {
  return window.matchMedia?.(DARK_QUERY).matches ?? false;
}
