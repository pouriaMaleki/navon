import { observer } from "mobx-react-lite";
import type { RootStore } from "../../../app/RootStore.js";
import { useAutoStartRoute } from "../hooks/useAutoStartRoute.js";
import { useBottomOverlayMeasurement } from "../hooks/useBottomOverlayMeasurement.js";
import { useCameraFollowTrigger } from "../hooks/useCameraFollowTrigger.js";
import { useFirstFixRecenter } from "../hooks/useFirstFixRecenter.js";
import { useHomePresentationReveal } from "../hooks/useHomePresentationReveal.js";
import { MapSurface } from "../MapSurface.js";
import { BottomOverlay } from "./BottomOverlay.js";
import { LeftSideRail } from "./LeftSideRail.js";
import { LocatingIndicator } from "./LocatingIndicator.js";
import { LocationBanner } from "./LocationBanner.js";
import { RightSideRail } from "./RightSideRail.js";
import { SpeedBadge } from "./SpeedBadge.js";
import { TopOverlay } from "./TopOverlay.js";

type Props = { store: RootStore };

export const HomeView = observer(({ store }: Props) => {
  useCameraFollowTrigger(store);
  useFirstFixRecenter(store);
  useHomePresentationReveal(store);
  useAutoStartRoute(store);
  useBottomOverlayMeasurement(store);

  return (
    <>
      <MapSurface store={store} />
      <TopOverlay store={store} />
      <BottomOverlay store={store} />
      <LocatingIndicator store={store} />
      <RightSideRail store={store} />
      <LeftSideRail store={store} />
      <SpeedBadge store={store} />
      <LocationBanner store={store} />
    </>
  );
});
