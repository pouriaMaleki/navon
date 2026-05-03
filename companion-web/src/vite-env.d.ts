interface ImportMetaEnv {
  readonly VITE_APP_VERSION: string;
  readonly VITE_APP_GIT_HASH: string;
  readonly VITE_APP_GIT_TIME: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
