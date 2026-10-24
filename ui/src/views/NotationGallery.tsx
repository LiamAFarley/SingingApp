// Browser-only check page for the notation renderer (`?scenario=notation`): generated melodies
// in different clefs, keys and meters.

import type { Score } from "../bindings/Score";
import gallery from "../notation/gallery.json";
import { describeKey } from "../notation/layout";
import { Staff } from "../notation/Staff";

export function NotationGallery() {
  const scores = gallery as unknown as Score[];
  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Notation</h1>
          <div className="sub">Generated melodies as the sight-singing screen draws them.</div>
        </div>
      </div>
      {scores.map((s, i) => (
        <div key={i} className="card section">
          <div className="small muted">
            {describeKey(s)} · {s.time} · {s.clef.replace("8vb", " 8vb")} · {s.bars} bars
          </div>
          <Staff score={s} maxHeight={380} />
        </div>
      ))}
    </div>
  );
}
