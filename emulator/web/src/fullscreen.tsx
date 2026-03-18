import { createRoot } from "react-dom/client";
import { installMobileGestureGuards } from "./mobile/installMobileGestureGuards";
import { AppStore } from "./stores/AppStore";
import { FullscreenApp } from "./ui/FullscreenApp";

const rootEl = document.getElementById("root");
if (!(rootEl instanceof HTMLDivElement)) {
  throw new Error("Expected #root div element");
}

installMobileGestureGuards();

const appStore = new AppStore();
createRoot(rootEl).render(<FullscreenApp appStore={appStore} />);
