import type { ReactElement } from "react";
import styles from "./TopBar.module.css";

export function TopBar(): ReactElement {
  return (
    <header className={styles["topbar"]}>
      <div className={styles["copy"]}>
        <h1 className={styles["title"]}>Minimap Emulator</h1>
        <p className={styles["subtitle"]}>Target profile: Waveshare ESP32-P4 LCD (800x800)</p>
      </div>
      <div className={styles["actions"]}>
        <a className={styles["linkButton"]} href="/fullscreen.html">
          Emulator Fullscreen
        </a>
        <a className={styles["linkButton"]} href="/web-fullscreen.html">
          Web Fullscreen
        </a>
      </div>
    </header>
  );
}
