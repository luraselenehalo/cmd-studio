// The last net under the whole app (main.tsx puts it around <App/>; phase 3's shell keeps it): a
// render that throws shows the E_INTERNAL notice, with the texts of spec §5.9's row (MEANINGS in
// app/src-tauri/src/error.rs) through `toIpcError` and what was thrown as the details, never a
// blank window, and Reload as the way out. It sits outside App, whose I18nProvider holds the
// chosen language, so the language is not known here: it shows the English and the Thai text
// together, each marked with its `lang` (round 2, fix 3).
import { Component, type ErrorInfo, type ReactNode } from "react";
import { fill } from "../i18n";
import { en } from "../i18n/en";
import { th } from "../i18n/th";
import { toIpcError } from "../ipc/types";

interface State {
  /** Something below threw while rendering; `error` is what it threw (any value, even null). */
  failed: boolean;
  error: unknown;
}

function reloadPage(): void {
  window.location.reload();
}

/** Two texts of the dictionaries, the English one first. */
function Both({ text }: { text: (dictionary: typeof en | typeof th) => string }) {
  return (
    <>
      <span lang="en">{text(en)}</span> · <span lang="th">{text(th)}</span>
    </>
  );
}

export class ErrorBoundary extends Component<{ children: ReactNode; /** For tests. */ reload?: () => void }, State> {
  override state: State = { failed: false, error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { failed: true, error };
  }

  /** On the console as well, with where it happened, for whoever looks into it. */
  override componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error("CMD Studio: a screen could not be drawn", error, info.componentStack ?? "");
  }

  override render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    const e = toIpcError(this.state.error);
    return (
      <div style={{ padding: "2rem" }}>
        <div className="notice error" role="alert">
          <strong>
            <Both text={(d) => d["error.title"]} />
          </strong>
          <p lang="en">{e.messageEn}</p>
          <p lang="th">{e.messageTh}</p>
          <details>
            <summary>
              <Both text={(d) => d["common.details"]} /> · <Both text={(d) => fill(d["error.code"], { code: e.code })} />
            </summary>
            {e.details && <pre>{e.details}</pre>}
          </details>
        </div>
        <div className="row-actions">
          <button type="button" className="primary" onClick={this.props.reload ?? reloadPage}>
            <Both text={(d) => d["error.reload"]} />
          </button>
        </div>
      </div>
    );
  }
}
