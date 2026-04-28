'use client';

import React, { useState } from 'react';
import { Eye, EyeOff, ChevronDown, ChevronRight } from 'lucide-react';
import type { Transcript } from '@/types';

interface Props {
  transcripts: Transcript[];
}

export function RawTranscriptToggle({ transcripts }: Props) {
  const [showRaw, setShowRaw] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const rawTranscripts = transcripts.filter(t => t.raw_text && t.raw_text !== t.text);

  if (rawTranscripts.length === 0) return null;

  return (
    <div className="border-t pt-3 mt-3">
      <button
        onClick={() => { setShowRaw(!showRaw); if (showRaw) setExpanded(false); }}
        className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        {showRaw ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
        {showRaw ? 'Hide' : 'Show'} Raw STT Output ({rawTranscripts.length} segments with corrections)
      </button>

      {showRaw && (
        <div className="mt-2 space-y-1">
          <button
            onClick={() => setExpanded(!expanded)}
            className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          >
            {expanded ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
            Raw STT output (pre-correction)
          </button>

          {expanded && (
            <div className="bg-muted/30 rounded-md p-3 max-h-64 overflow-y-auto font-mono text-xs space-y-2">
              {rawTranscripts.map((t, i) => (
                <div key={t.id || i} className="border-b border-border/50 pb-1 last:border-b-0">
                  <div className="text-muted-foreground line-through decoration-muted-foreground/50">
                    {t.raw_text}
                  </div>
                  <div className="text-foreground mt-0.5">
                    → {t.text}
                  </div>
                  {t.corrections_applied && (
                    <span className="text-[10px] text-primary">
                      {t.corrections_applied} correction(s)
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
