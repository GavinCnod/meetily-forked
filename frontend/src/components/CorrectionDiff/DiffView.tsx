'use client';

import React, { useEffect, useState, useCallback, useMemo } from 'react';
import { Check, X, AlertTriangle, ChevronDown, ChevronRight, RefreshCw, FileDown } from 'lucide-react';
import { terminologyService } from '@/services/terminologyService';
import type { TranscriptCorrection } from '@/types';

interface Props {
  meetingId: string;
  onCorrectionAccepted?: () => void;
  onRequestRegenerateSummary?: () => void;
}

interface TermGroup {
  original_span: string;
  suggestions: TranscriptCorrection[];
  risk: 'high' | 'medium' | 'low';
  totalCount: number;
}

const HIGH_RISK_TYPES = ['ghs_code', 'cas_number', 'un_number'];

export function CorrectionDiffView({ meetingId, onCorrectionAccepted, onRequestRegenerateSummary }: Props) {
  const [corrections, setCorrections] = useState<TranscriptCorrection[]>([]);
  const [applying, setApplying] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [loading, setLoading] = useState(true);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [accepting, setAccepting] = useState<Set<string>>(new Set());

  const fetchCorrections = useCallback(async () => {
    try {
      const list = await terminologyService.getCorrections(meetingId);
      setCorrections(list);
    } catch {
      // No corrections yet
    } finally {
      setLoading(false);
    }
  }, [meetingId]);

  useEffect(() => {
    fetchCorrections();
  }, [fetchCorrections]);

  // Group corrections by original_span for clustering
  const groups = useMemo(() => {
    const map = new Map<string, TranscriptCorrection[]>();
    for (const c of corrections) {
      if (c.status !== 'pending') continue;
      const key = c.original_span;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(c);
    }

    const result: TermGroup[] = [];
    for (const [, group] of map) {
      const first = group[0];
      const original = first.original_span;
      const isHighRisk = HIGH_RISK_TYPES.includes(first.correction_type);
      const isShort = original.length < 3;

      let risk: TermGroup['risk'] = 'low';
      if (isHighRisk) risk = 'high';
      else if (isShort) risk = 'medium';

      result.push({
        original_span: original,
        suggestions: group,
        risk,
        totalCount: group.length,
      });
    }
    return result.sort((a, b) => {
      const riskOrder = { high: 0, medium: 1, low: 2 };
      return riskOrder[a.risk] - riskOrder[b.risk];
    });
  }, [corrections]);

  const toggleGroup = (key: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const handleAcceptGroup = async (group: TermGroup) => {
    setAccepting(prev => new Set(prev).add(group.original_span));
    try {
      await terminologyService.acceptCorrectionForTerm(meetingId, group.original_span);
      await fetchCorrections();
      onCorrectionAccepted?.();
    } finally {
      setAccepting(prev => {
        const next = new Set(prev);
        next.delete(group.original_span);
        return next;
      });
    }
  };

  const handleRejectGroup = async (group: TermGroup) => {
    try {
      for (const s of group.suggestions) {
        await terminologyService.rejectCorrection(s.id);
      }
      await fetchCorrections();
    } catch {
      // ignore
    }
  };

  const handleAcceptSingle = async (correctionId: string) => {
    try {
      await terminologyService.acceptCorrection(correctionId);
      await fetchCorrections();
    } catch {
      // ignore
    }
  };

  const handleRejectSingle = async (correctionId: string) => {
    try {
      await terminologyService.rejectCorrection(correctionId);
      await fetchCorrections();
    } catch {
      // ignore
    }
  };

  const handleApplyAndRegenerate = async () => {
    setApplying(true);
    try {
      await terminologyService.applyAcceptedCorrections(meetingId);
      await fetchCorrections();
      onRequestRegenerateSummary?.();
    } catch {
      // ignore
    } finally {
      setApplying(false);
    }
  };

  const handleExportAudit = async () => {
    setExporting(true);
    try {
      const report = await terminologyService.exportAuditReport(meetingId);
      const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `audit-${meetingId}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      // ignore
    } finally {
      setExporting(false);
    }
  };

  const acceptedCount = corrections.filter(c => c.status === 'accepted').length;
  const pendingCount = corrections.filter(c => c.status === 'pending').length;

  if (loading) {
    return <div className="text-sm text-muted-foreground p-4">Loading corrections...</div>;
  }

  if (corrections.length === 0) {
    return null;
  }

  if (pendingCount === 0) {
    return (
      <div className="text-sm text-muted-foreground p-4">
        All {corrections.length} corrections have been reviewed.
      </div>
    );
  }

  return (
    <div className="space-y-3 p-1">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-medium">
          <AlertTriangle className="w-4 h-4 text-amber-500" />
          L3 Correction ({pendingCount} pending{acceptedCount > 0 ? `, ${acceptedCount} accepted` : ''})
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleExportAudit}
            disabled={exporting}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-xs border hover:bg-muted transition-colors"
          >
            <FileDown className="w-3 h-3" />
            {exporting ? 'Exporting...' : 'Audit Report'}
          </button>
          {acceptedCount > 0 && (
            <button
              onClick={handleApplyAndRegenerate}
              disabled={applying}
              className="flex items-center gap-1 px-2 py-0.5 rounded text-xs bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
            >
              <RefreshCw className={`w-3 h-3 ${applying ? 'animate-spin' : ''}`} />
              {applying ? 'Applying...' : 'Apply & Regenerate Summary'}
            </button>
          )}
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        The following suggestions are based on full-text matching. Review each occurrence carefully.
      </p>

      {groups.map(group => {
        const isExpanded = expandedGroups.has(group.original_span);
        const isAccepting = accepting.has(group.original_span);
        const bulkDisabled = group.risk === 'high' || group.original_span.length < 3;

        return (
          <div
            key={group.original_span}
            className={`border rounded-lg overflow-hidden ${
              group.risk === 'high' ? 'border-destructive/50 bg-destructive/5' :
              group.risk === 'medium' ? 'border-orange-200 bg-orange-50/50' :
              'border-border'
            }`}
          >
            {/* Group Header */}
            <div
              className="flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-muted/30 transition-colors"
              onClick={() => toggleGroup(group.original_span)}
            >
              {isExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
              <span className={`inline-block w-2 h-2 rounded-full ${
                group.risk === 'high' ? 'bg-destructive' :
                group.risk === 'medium' ? 'bg-orange-500' :
                'bg-green-500'
              }`} />
              <span className="font-mono text-sm font-medium">
                &ldquo;{group.original_span}&rdquo; → &ldquo;{group.suggestions[0].suggested_text}&rdquo;
              </span>
              <span className="text-xs text-muted-foreground ml-auto">
                ({group.totalCount} occurrence{group.totalCount > 1 ? 's' : ''})
              </span>

              {/* Action buttons on header */}
              <div className="flex items-center gap-1" onClick={e => e.stopPropagation()}>
                {bulkDisabled ? (
                  <span className="text-[10px] text-muted-foreground px-1">Review individually</span>
                ) : (
                  <button
                    onClick={() => handleAcceptGroup(group)}
                    disabled={isAccepting}
                    className="flex items-center gap-0.5 px-2 py-0.5 rounded text-xs bg-green-100 text-green-700 hover:bg-green-200 transition-colors"
                  >
                    <Check className="w-3 h-3" />
                    Accept all
                  </button>
                )}
                <button
                  onClick={() => handleRejectGroup(group)}
                  className="flex items-center gap-0.5 px-2 py-0.5 rounded text-xs bg-muted hover:bg-muted/80 transition-colors"
                >
                  <X className="w-3 h-3" />
                  Reject all
                </button>
              </div>
            </div>

            {/* Risk warnings */}
            {group.risk === 'high' && (
              <div className="px-3 py-1 text-xs text-destructive bg-destructive/10 flex items-center gap-1">
                <AlertTriangle className="w-3 h-3" />
                Safety-critical code. Must review each occurrence individually.
              </div>
            )}
            {group.original_span.length < 3 && (
              <div className="px-3 py-1 text-xs text-orange-600 bg-orange-100 flex items-center gap-1">
                <AlertTriangle className="w-3 h-3" />
                Short span (&lt;3 chars). Bulk accept disabled to prevent false matches.
              </div>
            )}

            {/* Expanded individual items */}
            {isExpanded && (
              <div className="border-t divide-y">
                {group.suggestions.map(s => (
                  <div key={s.id} className="px-4 py-2 flex items-center gap-3 text-sm">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-destructive line-through decoration-destructive text-xs">
                          {s.original_span}
                        </span>
                        <span className="text-muted-foreground">→</span>
                        <span className="font-mono text-green-700 text-xs">
                          {s.suggested_text}
                        </span>
                      </div>
                      {s.reason && (
                        <p className="text-xs text-muted-foreground mt-0.5">{s.reason}</p>
                      )}
                      <div className="flex items-center gap-2 mt-0.5">
                        <span className="text-[10px] text-muted-foreground">{s.language}</span>
                        <span className="text-[10px] text-muted-foreground">{s.correction_type}</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => handleAcceptSingle(s.id)}
                        className="p-1 rounded hover:bg-green-100 text-green-600 transition-colors"
                        title="Accept"
                      >
                        <Check className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={() => handleRejectSingle(s.id)}
                        className="p-1 rounded hover:bg-destructive/10 text-destructive transition-colors"
                        title="Reject"
                      >
                        <X className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
