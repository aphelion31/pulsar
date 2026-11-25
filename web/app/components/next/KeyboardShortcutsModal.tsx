import { X } from "lucide-react";

const SHORTCUTS = [
  { keys: ["⌘", "K"], desc: "Open command bar" },
  { keys: ["⌘", "↵"], desc: "Send reply / Add private note" },
  { keys: ["/"], desc: "Insert canned response in reply editor" },
  { keys: ["@"], desc: "Mention teammate in private note" },
  { keys: ["Esc"], desc: "Close dialogs or command bar" },
  { keys: ["?"], desc: "Open keyboard shortcuts" },
];

export function KeyboardShortcutsModal({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-n-overlay-default px-4 backdrop-blur-[1px]"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard shortcuts"
    >
      <div
        className="w-full max-w-md rounded-xl border border-n-weak bg-n-solid-1 p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-n-slate-12">Keyboard shortcuts</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close shortcuts"
            className="grid size-7 place-items-center rounded-lg text-n-slate-10 hover:bg-n-alpha-2 hover:text-n-slate-12"
          >
            <X className="size-4" />
          </button>
        </div>
        <ul className="m-0 list-none divide-y divide-n-weak">
          {SHORTCUTS.map((s) => (
            <li key={s.desc} className="flex items-center justify-between py-2.5 text-sm">
              <span className="text-n-slate-11">{s.desc}</span>
              <span className="flex items-center gap-1">
                {s.keys.map((k) => (
                  <kbd
                    key={k}
                    className="rounded border border-n-weak bg-n-solid-2 px-1.5 py-0.5 font-mono text-xs text-n-slate-12"
                  >
                    {k}
                  </kbd>
                ))}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
