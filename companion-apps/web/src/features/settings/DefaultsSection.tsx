import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import {
  ROUTE_SOURCE_MODE_DISPLAY_NAME,
  ROUTE_START_BEHAVIOR_DISPLAY_NAME,
  ROUTE_SUGGESTION_MODE_DISPLAY_NAME,
  type RouteSourceMode,
  type RouteStartBehavior,
  type RouteSuggestionMode,
} from "../../domain/models.js";
import styles from "./RoutePlannerSettingsView.module.css";

type Props = { store: RootStore };

export const DefaultsSection = observer(({ store }: Props) => {
  const planner = store.settingsStore.plannerPreferences;

  return (
    <div className={styles.section}>
      <h2>Defaults</h2>
      {store.planningStore.availableSourceModes.length > 1 ? (
        <div className={styles.row}>
          <div>
            <div className={styles.label}>Default route source</div>
            <div className={styles.hint}>Used for new planning sessions.</div>
          </div>
          <select
            className={styles.textInput}
            style={{ maxWidth: 180 }}
            value={planner.defaultSourceMode}
            onChange={(event) =>
              store.settingsStore.updatePlannerPreferences({
                defaultSourceMode: event.target.value as RouteSourceMode,
              })
            }
          >
            {store.planningStore.availableSourceModes.map((mode) => (
              <option key={mode} value={mode}>
                {ROUTE_SOURCE_MODE_DISPLAY_NAME[mode]}
              </option>
            ))}
          </select>
        </div>
      ) : null}

      <div className={styles.row}>
        <div>
          <div className={styles.label}>Suggestions</div>
          <div className={styles.hint}>Best route only or up to three.</div>
        </div>
        <select
          className={styles.textInput}
          style={{ maxWidth: 180 }}
          value={planner.suggestionMode}
          onChange={(event) =>
            store.settingsStore.updatePlannerPreferences({
              suggestionMode: event.target.value as RouteSuggestionMode,
            })
          }
        >
          <option value="bestOnly">{ROUTE_SUGGESTION_MODE_DISPLAY_NAME.bestOnly}</option>
          <option value="threeRoutes">{ROUTE_SUGGESTION_MODE_DISPLAY_NAME.threeRoutes}</option>
        </select>
      </div>

      <div className={styles.row}>
        <div>
          <div className={styles.label}>Start behavior</div>
          <div className={styles.hint}>When picking a recent route or import.</div>
        </div>
        <select
          className={styles.textInput}
          style={{ maxWidth: 180 }}
          value={planner.startBehavior}
          onChange={(event) =>
            store.settingsStore.updatePlannerPreferences({
              startBehavior: event.target.value as RouteStartBehavior,
            })
          }
        >
          <option value="explicit">{ROUTE_START_BEHAVIOR_DISPLAY_NAME.explicit}</option>
          <option value="automatic">{ROUTE_START_BEHAVIOR_DISPLAY_NAME.automatic}</option>
        </select>
      </div>
    </div>
  );
});
