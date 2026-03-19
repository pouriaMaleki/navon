import { createRoot } from "react-dom/client";
import { installMobileGestureGuards } from "./mobile/installMobileGestureGuards";
import { AppStore } from "./stores/AppStore";
import { App } from "./ui/App";

const rootEl = document.getElementById("root");
if (!(rootEl instanceof HTMLDivElement)) {
  throw new Error("Expected #root div element");
}

installMobileGestureGuards();

const appStore = new AppStore({ autoRequestLiveGps: false });
createRoot(rootEl).render(<App appStore={appStore} />);
