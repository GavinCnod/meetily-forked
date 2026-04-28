/**
 * Terminology Service
 * Wraps Tauri backend calls for terminology management and L3 corrections.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  TerminologyEntry,
  TerminologySettings,
  L3CorrectionJob,
  TranscriptCorrection,
} from '@/types';

export class TerminologyService {
  // ── Terminology CRUD ──

  async getList(): Promise<TerminologyEntry[]> {
    return invoke<TerminologyEntry[]>('get_terminology_list');
  }

  async saveEntry(entry: {
    id?: string;
    original: string;
    replacement: string;
    language: string;
    case_sensitive: boolean;
    whole_word: boolean;
    enabled: boolean;
    priority: string;
    category: string;
    description?: string;
  }): Promise<TerminologyEntry> {
    return invoke<TerminologyEntry>('save_terminology_entry', { ...entry });
  }

  async deleteEntry(id: string): Promise<boolean> {
    return invoke<boolean>('delete_terminology_entry', { id });
  }

  // ── CSV Import ──

  async importCSV(csvContent: string): Promise<{
    batch_id: string;
    new_count: number;
    updated_count: number;
    errors: string[];
  }> {
    return invoke('import_terminology_csv', { csvContent });
  }

  // ── Save with Terminology ──

  async saveMeetingWithTerminology(
    meetingTitle: string,
    transcripts: any[],
    folderPath: string | null,
    snapshotHash: string | null,
    l1PromptSnapshot: string | null,
  ): Promise<{ status: string; message: string; meeting_id: string }> {
    return invoke('save_transcript_with_terminology', {
      meetingTitle,
      transcripts,
      folderPath,
      terminologySnapshotHash: snapshotHash,
      l1PromptSnapshot,
    });
  }

  // ── Package Management ──

  async enablePackage(packageId: string, enabled: boolean): Promise<number> {
    return invoke<number>('enable_package', { packageId, enabled });
  }

  async disablePackage(packageId: string): Promise<number> {
    return invoke<number>('disable_package', { packageId });
  }

  // ── Cache & Snapshot ──

  async refreshCache(): Promise<void> {
    return invoke<void>('refresh_all_terminology_caches');
  }

  // ── Settings ──

  async getSettings(): Promise<TerminologySettings> {
    return invoke<TerminologySettings>('get_terminology_settings');
  }

  async setSettings(settings: {
    terminology_enabled?: boolean;
    initial_prompt_enabled?: boolean;
    llm_correction_enabled?: boolean;
  }): Promise<void> {
    return invoke<void>('set_terminology_settings', settings);
  }

  // ── L3 Queue ──

  async runL3Correction(meetingId: string): Promise<string> {
    return invoke<string>('run_llm_terminology_correction', { meetingId });
  }

  async getL3JobStatus(meetingId: string): Promise<L3CorrectionJob | null> {
    return invoke<L3CorrectionJob | null>('get_l3_job_status', { meetingId });
  }

  async retryL3Correction(meetingId: string): Promise<string> {
    return invoke<string>('retry_l3_correction', { meetingId });
  }

  // ── L3 Review ──

  async getCorrections(meetingId: string): Promise<TranscriptCorrection[]> {
    return invoke<TranscriptCorrection[]>('get_corrections_for_meeting', { meetingId });
  }

  async acceptCorrection(correctionId: string): Promise<void> {
    return invoke<void>('accept_correction', { correctionId });
  }

  async acceptCorrectionForTerm(meetingId: string, originalSpan: string): Promise<number> {
    return invoke<number>('accept_correction_for_term', { meetingId, originalSpan });
  }

  async rejectCorrection(correctionId: string): Promise<void> {
    return invoke<void>('reject_correction', { correctionId });
  }
}

export const terminologyService = new TerminologyService();
