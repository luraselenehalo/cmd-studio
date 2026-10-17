// The last net under the whole app (main.tsx puts it around <App/>; phase 3's shell keeps it): a
// render that throws shows the E_INTERNAL notice, with the texts of spec §5.9's row (MEANINGS in
// app/src-tauri/src/error.rs) through `toIpcError` and what was thrown as the details, never a
// blank window. It sits outside App, whose I18nProvider holds the chosen language, so it brings
// its own and speaks English, the language the app opens in (founder 2026-10-16).
import { Component, type ReactNode } from "react";
import { I18nProvider } from "../i18n";
import { toIpcError } from "../ipc/types";
import { ErrorNotice } from "./Notices";

interface State {
  /** Something below threw while rendering; `error` is what it threw (any value, even null). */
  failed: boolean;
  error: unknown;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  override state: State = { failed: false, error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { failed: true, error };
  }

  override render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <I18nProvider language="en" setLanguage={() => {}}>
        <div lang="en" style={{ padding: "2rem" }}>
          <ErrorNotice error={toIpcError(this.state.error)} />
        </div>
      </I18nProvider>
    );
  }
}
