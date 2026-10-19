// Menu of a signature flag on the ruler (`tempoPointer.ts` opens it): another signature from this bar, or remove the change.
import { For, Show } from "solid-js";
import type { TimeSignature } from "../api/types";
import { barIndex } from "../common/tempoMap";
import { t } from "../i18n";
import { useSession } from "../session";
import { ContextMenu, MenuItem, MenuLabel } from "../common/ContextMenu";
import { signatureLabel } from "./tempoLayout";
import type { TempoMenu as Menu } from "./tempoPointer";

/** Signatures offered in the menu; any other is set with the transport bar fields (they edit the signature at the playhead). */
const SIGNATURES: readonly TimeSignature[] = [
  { numerator: 2, denominator: 4 }, { numerator: 3, denominator: 4 }, { numerator: 4, denominator: 4 }, { numerator: 5, denominator: 4 },
  { numerator: 6, denominator: 8 }, { numerator: 7, denominator: 8 }, { numerator: 9, denominator: 8 }, { numerator: 12, denominator: 8 },
];

export function TempoMenu(props: { menu: Menu; onClose: () => void }) {
  const session = useSession();
  const tempo = session.tempo;
  const map = () => session.project.tempoMap;
  const bar = (tick: number) => barIndex(map(), tick) + 1;
  const close = props.onClose;
  const msg = () => t().tempo.menu;

  const change = () => map().signatures[props.menu.index];
  return (
    <Show when={change()}>
      {(c) => (
        <ContextMenu at={props.menu.at} label={msg().signatureChange} onClose={close}>
          <MenuLabel>{msg().signatureAt(bar(c().tick), signatureLabel(c().signature))}</MenuLabel>
          <For each={SIGNATURES}>
            {(s) => <MenuItem onClose={close} onPick={() => void tempo.setSignatureFrom(c().tick, s)}>{signatureLabel(s)}</MenuItem>}
          </For>
          <Show when={props.menu.index > 0}>
            <MenuItem onClose={close} onPick={() => void tempo.removeSignature(props.menu.index)}>{msg().removeSignature}</MenuItem>
          </Show>
        </ContextMenu>
      )}
    </Show>
  );
}
