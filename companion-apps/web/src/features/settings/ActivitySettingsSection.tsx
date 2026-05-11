import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { useT } from "../../i18n/useT.js";
import { BackgroundGpsService } from "../../integrations/permissions/BackgroundGpsService.js";
import { platformGpsHintKey } from "./PlatformHints.js";

type Props = { store: RootStore };

const backgroundGpsService = new BackgroundGpsService();

/**
 * Settings block rendered at the very top of the settings hub. Contains, in
 * spec order (docs/ux-specs.md lines 128-145):
 *   1. Prevent screen from turning off
 *   2. Allow GPS in background
 *   3. Audio cues during routing (gated on #2)
 *   4. Live actions / map on lock screen (gated on #2)
 */
export const ActivitySettingsSection = observer(({ store }: Props) => {
  const t = useT(store);
  const settings = store.settingsStore.settings;
  const gpsOn = settings.allowBackgroundGps;

  const onToggleBackgroundGps = async (next: boolean) => {
    store.settingsStore.updateSettings({ allowBackgroundGps: next });
    if (next) {
      await backgroundGpsService.requestPermission();
    }
  };

  return (
    <div className="settings-section" data-testid="activity-settings">
      <ToggleRow
        testId="setting-keepScreenOn"
        title={t("settings.activity.keepScreenOn.title")}
        subtitle={t("settings.activity.keepScreenOn.subtitle")}
        checked={settings.keepScreenOn}
        onChange={(v) => store.settingsStore.updateSettings({ keepScreenOn: v })}
      />
      <ToggleRow
        testId="setting-allowBackgroundGps"
        title={t("settings.activity.allowBackgroundGps.title")}
        subtitle={t(platformGpsHintKey())}
        checked={settings.allowBackgroundGps}
        onChange={onToggleBackgroundGps}
      />
      <ToggleRow
        testId="setting-audioCuesEnabled"
        title={t("settings.activity.audioCues.title")}
        subtitle={
          gpsOn
            ? t("settings.activity.audioCues.subtitle")
            : "Requires GPS in background. Turn that on first."
        }
        checked={settings.audioCuesEnabled}
        disabled={!gpsOn}
        onChange={(v) => store.settingsStore.updateSettings({ audioCuesEnabled: v })}
      />
      <ToggleRow
        testId="subsetting-audioCuesOnlyInBackground"
        title={t("settings.activity.audioCuesOnlyInBackground.title")}
        subtitle={t("settings.activity.audioCuesOnlyInBackground.subtitle")}
        checked={settings.audioCuesOnlyInBackground}
        disabled={!gpsOn || !settings.audioCuesEnabled}
        onChange={(v) => store.settingsStore.updateSettings({ audioCuesOnlyInBackground: v })}
      />
      <ToggleRow
        testId="setting-liveActivityEnabled"
        title={t("settings.activity.liveActivity.title")}
        subtitle={
          gpsOn
            ? t("settings.activity.liveActivity.subtitle")
            : "Requires GPS in background. Turn that on first."
        }
        checked={settings.liveActivityEnabled}
        disabled={!gpsOn}
        onChange={(v) => store.settingsStore.updateSettings({ liveActivityEnabled: v })}
      />
      <ToggleRow
        testId="setting-routingDiagnosticsEnabled"
        title="Record routing diagnostics"
        subtitle="Captures GPS, routes, cues, and UI events into shareable debug sessions."
        checked={settings.routingDiagnosticsEnabled}
        onChange={(v) => {
          store.settingsStore.updateSettings({ routingDiagnosticsEnabled: v });
          if (v) {
            store.routingDiagnosticsStore.startRecording();
          } else {
            store.routingDiagnosticsStore.stopRecording();
          }
        }}
      />
    </div>
  );
});

type ToggleRowProps = {
  testId: string;
  title: string;
  subtitle: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
};

function ToggleRow({ testId, title, subtitle, checked, disabled, onChange }: ToggleRowProps) {
  return (
    <label
      className="list-row"
      data-testid={testId}
      data-disabled={disabled ? "true" : "false"}
      style={{ opacity: disabled ? 0.5 : 1 }}
    >
      <div style={{ flex: 1, textAlign: "start" }}>
        <div className="list-row__title">{title}</div>
        <div className="list-row__subtitle">{subtitle}</div>
      </div>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-disabled={disabled ? "true" : "false"}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
