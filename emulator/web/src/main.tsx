import { createRoot } from "react-dom/client";
import { AppStore } from "./stores/AppStore";
import { App } from "./ui/App";

const rootEl = document.getElementById("root");
if (!(rootEl instanceof HTMLDivElement)) {
  throw new Error("Expected #root div element");
}

const appStore = new AppStore();
createRoot(rootEl).render(<App appStore={appStore} />);
