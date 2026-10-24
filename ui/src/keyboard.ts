// Components that take over the keyboard (the score editor) register here; the session screen
// offers each key to them before applying its own shortcuts.

import { useEffect, useRef } from "react";

type Handler = (e: KeyboardEvent) => boolean;

const handlers: { current: Handler }[] = [];

/** Offers a key to registered owners, newest first; true when one of them handled it. */
export function offerKey(e: KeyboardEvent): boolean {
  for (let i = handlers.length - 1; i >= 0; i--) {
    if (handlers[i].current(e)) return true;
  }
  return false;
}

/** Registers `handler` while `active`; it returns true for keys it consumed. */
export function useKeyOwner(handler: Handler, active = true) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    if (!active) return;
    const entry = { current: (e: KeyboardEvent) => ref.current(e) };
    handlers.push(entry);
    return () => {
      const i = handlers.indexOf(entry);
      if (i >= 0) handlers.splice(i, 1);
    };
  }, [active]);
}
