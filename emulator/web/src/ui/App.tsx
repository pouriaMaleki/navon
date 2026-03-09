import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import type { AppStore } from "../stores/AppStore";
import styles from "./App.module.css";
import { Controls } from "./Controls";
import { EmulatorPanel } from "./EmulatorPanel";
import { TopBar } from "./TopBar";

type AppProps = {
  appStore: AppStore;
};

export const App = observer(({ appStore }: AppProps) => {
  useEffect(() => {
    return () => {
      appStore.dispose();
    };
  }, [appStore]);

  return (
    <main className={styles["app"]}>
      <TopBar />
      <EmulatorPanel emulatorStore={appStore.emulatorStore} />
      <Controls appStore={appStore} />
    </main>
  );
});
