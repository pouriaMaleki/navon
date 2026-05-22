import { observer } from "mobx-react-lite";
import type { RootStore } from "../../app/RootStore.js";
import { DefaultsSection } from "./DefaultsSection.js";
import { HslSection } from "./HslSection.js";
import { RidingSection } from "./RidingSection.js";

type Props = { store: RootStore };

export const RoutePlannerSettingsView = observer(({ store }: Props) => (
  <>
    <DefaultsSection store={store} />
    <RidingSection store={store} />
    <HslSection store={store} />
  </>
));
