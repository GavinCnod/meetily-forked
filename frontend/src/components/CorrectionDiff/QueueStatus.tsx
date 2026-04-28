'use client';

import React, { useEffect, useState, useCallback } from 'react';
import { RefreshCw, AlertCircle, CheckCircle, Clock, Loader2 } from 'lucide-react';
import { terminologyService } from '@/services/terminologyService';
import type { L3CorrectionJob, L3JobStatus } from '@/types';

interface Props {
  meetingId: string;
}

const STATUS_ICONS: Record<L3JobStatus, React.ReactNode> = {
  queued: <Clock className="w-4 h-4 text-muted-foreground" />,
  running: <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />,
  done: <CheckCircle className="w-4 h-4 text-green-500" />,
  failed: <AlertCircle className="w-4 h-4 text-destructive" />,
  timeout: <AlertCircle className="w-4 h-4 text-orange-500" />,
};

const STATUS_LABELS: Record<L3JobStatus, string> = {
  queued: 'Queued',
  running: 'Processing...',
  done: 'Complete',
  failed: 'Failed',
  timeout: 'Timed out',
};

export function L3QueueStatus({ meetingId }: Props) {
  const [job, setJob] = useState<L3CorrectionJob | null>(null);
  const [loading, setLoading] = useState(false);

  const fetchStatus = useCallback(async () => {
    try {
      const result = await terminologyService.getL3JobStatus(meetingId);
      setJob(result);
    } catch {
      // No job yet
    }
  }, [meetingId]);

  useEffect(() => {
    fetchStatus();
    // Poll while running/queued
    const interval = setInterval(fetchStatus, 3000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  const handleRetry = async () => {
    setLoading(true);
    try {
      await terminologyService.retryL3Correction(meetingId);
      await fetchStatus();
    } finally {
      setLoading(false);
    }
  };

  if (!job) return null;

  const status = job.status as L3JobStatus;

  return (
    <div className="flex items-center gap-2 px-3 py-2 rounded-lg border bg-muted/20 text-sm">
      {STATUS_ICONS[status] || <Clock className="w-4 h-4" />}
      <span className="font-medium">L3 Correction:</span>
      <span>{STATUS_LABELS[status] || status}</span>
      {(status === 'failed' || status === 'timeout') && (
        <button
          onClick={handleRetry}
          disabled={loading}
          className="ml-auto flex items-center gap-1 px-2 py-0.5 rounded bg-primary/10 text-primary hover:bg-primary/20 transition-colors text-xs"
        >
          <RefreshCw className={`w-3 h-3 ${loading ? 'animate-spin' : ''}`} />
          Retry
        </button>
      )}
    </div>
  );
}
