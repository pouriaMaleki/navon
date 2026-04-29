import { observer } from "mobx-react-lite";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { HomeView } from "../features/home/HomeView.js";
import { WebTtsService } from "../integrations/audio/WebTtsService.js";
import { startRoutingActivityCoordinator } from "../integrations/cues/RoutingActivityCoordinator.js";
import { LiveNotificationService } from "../integrations/notifications/LiveNotificationService.js";
import { WakeLockService } from "../integrations/screen/WakeLockService.js";
import type { RootStore } from "./RootStore.js";

const SettingsHubView = lazy(() =>
  import("../features/settings/SettingsHubView.js").then((m) => ({ default: m.SettingsHubView })),
);

type Props = { store: RootStore };

export const AppShell = observer(({ store }: Props) => {
  const [isDragging, setIsDragging] = useState(false);

  const handleDragOver = useCallback((event: DragEvent) => {
    if (event.dataTransfer?.types.includes("Files")) {
      event.preventDefault();
      setIsDragging(true);
    }
  }, []);

  const handleDragLeave = useCallback((event: DragEvent) => {
    if (event.relatedTarget === null) {
      setIsDragging(false);
    }
  }, []);

  const handleDrop = useCallback(
    (event: DragEvent) => {
      event.preventDefault();
      setIsDragging(false);
      const file = event.dataTransfer?.files?.[0];
      if (!file) return;
      file.text().then((content) => {
        void store.ingestSharedImport({ kind: "file", fileName: file.name, content });
      });
    },
    [store],
  );

  useEffect(() => {
    window.addEventListener("dragover", handleDragOver);
    window.addEventListener("dragleave", handleDragLeave);
    window.addEventListener("drop", handleDrop);
    return () => {
      window.removeEventListener("dragover", handleDragOver);
      window.removeEventListener("dragleave", handleDragLeave);
      window.removeEventListener("drop", handleDrop);
    };
  }, [handleDragOver, handleDragLeave, handleDrop]);

  const servicesRef = useRef<{
    wakeLock: WakeLockService;
    tts: WebTtsService;
    liveNotification: LiveNotificationService;
  } | null>(null);
  if (!servicesRef.current) {
    servicesRef.current = {
      wakeLock: new WakeLockService(),
      tts: new WebTtsService(),
      liveNotification: new LiveNotificationService(),
    };
  }
  useEffect(() => {
    const services = servicesRef.current;
    if (!services) return;
    const dispose = startRoutingActivityCoordinator(store, services);
    return () => {
      dispose();
      services.wakeLock.dispose();
      services.liveNotification.stop();
    };
  }, [store]);

  return (
    <div className="app-shell">
      <HomeView store={store} />
      {store.route === "settings" ? (
        <Suspense fallback={<div className="settings-overlay" />}>
          <SettingsHubView store={store} />
        </Suspense>
      ) : null}
      {isDragging ? <div className="drop-overlay">Drop a .gpx file to import</div> : null}
    </div>
  );
});
