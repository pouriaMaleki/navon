import { createRoot } from "react-dom/client";
import { browserViewportProfile } from "./core/screenProfiles";
import { installMobileGestureGuards } from "./mobile/installMobileGestureGuards";
import { AppStore } from "./stores/AppStore";
import { WebFullscreenApp } from "./ui/WebFullscreenApp";

const rootEl = document.getElementById("root");
if (!(rootEl instanceof HTMLDivElement)) {
  throw new Error("Expected #root div element");
}

installMobileGestureGuards();

const appStore = new AppStore({ screenProfile: browserViewportProfile, autoRequestLiveGps: true });
createRoot(rootEl).render(<WebFullscreenApp appStore={appStore} />);
