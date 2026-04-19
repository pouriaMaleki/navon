import { observer } from "mobx-react-lite";
import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { HomeView } from "../features/home/HomeView.js";
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
