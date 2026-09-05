import { en } from "../i18n/en";
import { fill } from "../i18n/format";
import { type ModuleWork } from "../hooks/useModuleWork";

type ModuleWorkBandProps = { work: ModuleWork };

/**
 * What a module's own work looks like while it runs: a line, a progress, and a way to stop it.
 *
 * The Stop is not a registry command and is not drawn from one. It appears with the work and goes
 * with it, which is the shape `TranscribePanel` already draws its own cancel in, and the 2026-09-03
 * ruling says of itself that it is about commands and not panels.
 *
 * Pressing it is a request. A module that never asks the host whether it should stop, or asks and
 * ignores the answer, keeps running, so the button stays drawn rather than pretending otherwise.
 */
export default function ModuleWorkBand({ work }: ModuleWorkBandProps) {
  const { message, progress } = work;

  return (
    <section className="modulework">
      {/* The module's own line when it has set one, and the core's word for work otherwise. The
        core has no word for what any module does, so this one is about work in general. */}
      <p className="modulework__status">{message ?? en.modules.work.working}</p>
      {progress !== null && (
        <>
          <progress
            className="modulework__progress"
            value={progress.done}
            max={progress.total}
            aria-label={en.modules.work.working}
          />
          <span className="modulework__count">
            {fill(en.modules.work.count, { done: progress.done, total: progress.total })}
          </span>
        </>
      )}
      <button className="modulework__stop" type="button" onClick={() => work.stop()}>
        {en.modules.work.stop}
      </button>
    </section>
  );
}
