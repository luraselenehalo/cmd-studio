// Advanced (spec §4.3): the add-ons' versions, the data folder with the buttons that open it and
// its log folder, the log's tail, and the files CMD Studio generated, behind toggles and read
// only. The page of the technical names (spec §4.5's words table): Oh My Posh and Clink are named
// here, as they are in Help's credits and in the doctor's own messages. A read that failed is
// said with Check again, and a folder that did not open is said (spec §5.9).
import { useEffect, useState } from "react";
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import type { ExternalTarget } from "../bindings/ExternalTarget";
import { ErrorNotice } from "../components/Notices";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { useAppState } from "../state";

export function Advanced() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { version } = useAppState();
  const [info, setInfo] = useState<AdvancedInfo | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [openError, setOpenError] = useState<unknown>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let alive = true;
    ipc.advancedRead().then(
      (i) => {
        if (!alive) return;
        setInfo(i);
        setLoadError(null);
      },
      (e: unknown) => {
        if (alive) setLoadError(e);
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, version, tick]);

  const open = async (target: ExternalTarget) => {
    setOpenError(null);
    try {
      await ipc.openExternal(target);
    } catch (e) {
      setOpenError(e);
    }
  };

  return (
    <section>
      <h1 tabIndex={-1}>{t("advanced.title")}</h1>
      {loadError !== null && <ErrorNotice error={loadError} />}
      {!info && loadError === null && <p role="status">{t("common.loading")}</p>}
      {info && (
        <>
          <h2>{t("advanced.versions")}</h2>
          <table>
            <tbody>
              <tr>
                <th scope="row">{t("advanced.version.app")}</th>
                <td>{info.versions.app}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.ohMyPosh")}</th>
                <td>{info.versions.ohMyPosh}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.clink")}</th>
                <td>{info.versions.clink}</td>
              </tr>
              <tr>
                <th scope="row">{t("advanced.version.nerdFonts")}</th>
                <td>{info.versions.nerdFonts}</td>
              </tr>
            </tbody>
          </table>
        </>
      )}
      <h2>{t("advanced.dataDir")}</h2>
      {info && (
        <p>
          <code>{info.dataDir}</code>
        </p>
      )}
      {/* There even when the read failed: the folders open without it, and Check again reads anew. */}
      <div className="row-actions">
        <button type="button" onClick={() => void open("dataFolder")}>
          {t("advanced.openData")}
        </button>
        <button type="button" onClick={() => void open("logsFolder")}>
          {t("advanced.openLogs")}
        </button>
        <button type="button" onClick={() => setTick((n) => n + 1)}>
          {t("common.refresh")}
        </button>
      </div>
      {openError !== null && <ErrorNotice error={openError} />}
      {info && (
        <>
          <h2>{t("advanced.log")}</h2>
          <pre className="log">{info.logTail}</pre>
          <h2>{t("advanced.generated")}</h2>
          {info.generated.length === 0 && <p>{t("advanced.none")}</p>}
          {info.generated.map((file) => (
            <details key={file.name}>
              <summary>
                <code>{file.name}</code>
              </summary>
              <pre className="log">{file.text}</pre>
            </details>
          ))}
        </>
      )}
    </section>
  );
}
