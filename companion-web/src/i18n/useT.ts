// React hook that returns a `t(key, values?)` function bound to the user's
// language preference. The component must be wrapped in `observer` for
// MobX to track the access to `settings.language` and re-render when the
// preference changes.

import type { RootStore } from "../app/RootStore.js";
import { resolveLocale, tIn } from "./index.js";
import type { MessageValues } from "./messageFormat.js";

export type TFunction = (key: string, values?: MessageValues) => string;

export function useT(store: RootStore): TFunction {
  const locale = resolveLocale(store.settingsStore.settings.language);
  return (key, values) => tIn(locale, key, values);
}
