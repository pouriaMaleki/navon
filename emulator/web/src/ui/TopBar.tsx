import type { ReactElement } from "react";
import styles from "./TopBar.module.css";

export function TopBar(): ReactElement {
  return (
    <header className={styles["topbar"]}>
      <h1 className={styles["title"]}>Minimap Emulator</h1>
      <p className={styles["subtitle"]}>Target profile: Waveshare ESP32-P4 LCD (800x800)</p>
    </header>
  );
}
