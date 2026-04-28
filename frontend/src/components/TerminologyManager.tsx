'use client';

import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Plus, Pencil, Trash2, RefreshCw, Upload, BookOpen } from 'lucide-react';
import { motion } from 'framer-motion';
import { terminologyService } from '@/services/terminologyService';
import type { TerminologyEntry } from '@/types';

const LANGUAGES = ['auto', 'ja', 'zh', 'en', 'other'] as const;
const PRIORITIES = ['high', 'normal', 'low'] as const;
const CATEGORIES = ['general', 'chemical', 'ghs_code', 'cas_number', 'un_number', 'custom'] as const;

interface EditDialogState {
  open: boolean;
  entry: Partial<TerminologyEntry> | null;
}

export function TerminologyManager() {
  const [entries, setEntries] = useState<TerminologyEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editDialog, setEditDialog] = useState<EditDialogState>({ open: false, entry: null });
  const [packageFilter, setPackageFilter] = useState<string>('all');
  const [packages, setPackages] = useState<string[]>([]);
  const [importResult, setImportResult] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const loadEntries = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const list = await terminologyService.getList();
      setEntries(list);
      const pkgSet = new Set(list.filter(e => e.package_id).map(e => e.package_id!));
      setPackages(['all', ...Array.from(pkgSet)]);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  const filteredEntries = packageFilter === 'all'
    ? entries
    : entries.filter(e => e.package_id === packageFilter);

  const handleToggle = async (entry: TerminologyEntry) => {
    try {
      await terminologyService.saveEntry({
        id: entry.id,
        original: entry.original,
        replacement: entry.replacement,
        language: entry.language,
        case_sensitive: entry.case_sensitive !== 0,
        whole_word: entry.whole_word !== 0,
        enabled: entry.enabled === 0,
        priority: entry.priority,
        category: entry.category,
        description: entry.description || undefined,
      });
      await terminologyService.refreshCache();
      await loadEntries();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this terminology entry?')) return;
    try {
      await terminologyService.deleteEntry(id);
      await terminologyService.refreshCache();
      await loadEntries();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleSave = async () => {
    if (!editDialog.entry) return;
    const e = editDialog.entry;
    if (!e.original?.trim() || !e.replacement?.trim()) {
      setError('Original and Replacement are required');
      return;
    }
    try {
      await terminologyService.saveEntry({
        id: e.id,
        original: e.original,
        replacement: e.replacement,
        language: e.language || 'auto',
        case_sensitive: e.case_sensitive ?? false,
        whole_word: e.whole_word ?? true,
        enabled: e.enabled !== undefined ? e.enabled !== 0 : true,
        priority: e.priority || 'normal',
        category: e.category || 'general',
        description: e.description || undefined,
      });
      await terminologyService.refreshCache();
      await loadEntries();
      setEditDialog({ open: false, entry: null });
    } catch (err) {
      setError(String(err));
    }
  };

  const handleOpenNew = () => {
    setEditDialog({
      open: true,
      entry: {
        original: '',
        replacement: '',
        language: 'auto',
        case_sensitive: false,
        whole_word: true,
        enabled: true,
        priority: 'normal',
        category: 'general',
        description: '',
      } as any,
    });
  };

  const handleOpenEdit = (entry: TerminologyEntry) => {
    setEditDialog({ open: true, entry: { ...entry } });
  };

  const handleImportCSV = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const buffer = await file.arrayBuffer();
      const bytes = Array.from(new Uint8Array(buffer));
      const result = await terminologyService.importCSV(text, bytes);
      setImportResult(
        `Imported: ${result.new_count} new, ${result.updated_count} updated` +
        (result.errors.length > 0 ? `. ${result.errors.length} errors.` : '')
      );
      await terminologyService.refreshCache();
      await loadEntries();
    } catch (err) {
      setError(`Import failed: ${err}`);
    }
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <BookOpen className="w-5 h-5 text-primary" />
          <h2 className="text-lg font-semibold">Terminology Manager</h2>
          <span className="text-xs text-muted-foreground">
            {entries.filter(e => e.enabled !== 0).length}/{entries.length} active
          </span>
        </div>
        <div className="flex items-center gap-2">
          <input
            ref={fileInputRef}
            type="file"
            accept=".csv"
            onChange={handleImportCSV}
            className="hidden"
          />
          <button
            onClick={() => fileInputRef.current?.click()}
            className="flex items-center gap-1 px-2 py-1.5 rounded-md border text-sm hover:bg-muted transition-colors"
            title="Import CSV"
          >
            <Upload className="w-4 h-4" />
            Import
          </button>
          <button
            onClick={() => { terminologyService.refreshCache().then(loadEntries); }}
            className="p-2 rounded-md hover:bg-muted transition-colors"
            title="Refresh cache"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
          <button
            onClick={handleOpenNew}
            className="flex items-center gap-1 px-3 py-1.5 bg-primary text-primary-foreground rounded-md text-sm hover:opacity-90 transition-opacity"
          >
            <Plus className="w-4 h-4" />
            Add Term
          </button>
        </div>
      </div>

      {/* Package Filter */}
      {packages.length > 1 && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Package:</span>
          <select
            value={packageFilter}
            onChange={e => setPackageFilter(e.target.value)}
            className="text-xs border rounded px-2 py-1 bg-background"
          >
            {packages.map(pkg => (
              <option key={pkg} value={pkg}>
                {pkg === 'all' ? 'All Packages' : pkg}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="bg-destructive/10 text-destructive text-sm p-2 rounded-md flex justify-between">
          <span>{error}</span>
          <button onClick={() => setError(null)} className="font-bold">x</button>
        </div>
      )}

      {/* Import Result */}
      {importResult && (
        <div className="bg-primary/10 text-primary text-sm p-2 rounded-md flex justify-between">
          <span>{importResult}</span>
          <button onClick={() => setImportResult(null)} className="font-bold">x</button>
        </div>
      )}

      {/* Table */}
      <div className="border rounded-lg overflow-hidden">
        {loading ? (
          <div className="p-8 text-center text-muted-foreground">Loading...</div>
        ) : filteredEntries.length === 0 ? (
          <div className="p-8 text-center text-muted-foreground">
            <p>No terminology entries yet.</p>
            <button onClick={handleOpenNew} className="text-primary hover:underline text-sm mt-1">
              Add your first term
            </button>
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead className="bg-muted/50 border-b">
              <tr>
                <th className="text-left px-3 py-2 font-medium">Original</th>
                <th className="text-left px-3 py-2 font-medium">Replacement</th>
                <th className="text-left px-3 py-2 font-medium w-16">Lang</th>
                <th className="text-left px-3 py-2 font-medium w-16">Priority</th>
                <th className="text-center px-3 py-2 font-medium w-16">Active</th>
                <th className="text-right px-3 py-2 font-medium w-20">Actions</th>
              </tr>
            </thead>
            <tbody>
              {filteredEntries.map((entry, i) => (
                <motion.tr
                  key={entry.id}
                  initial={{ opacity: 0, y: 4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ delay: i * 0.02 }}
                  className="border-b last:border-b-0 hover:bg-muted/30 transition-colors"
                >
                  <td className="px-3 py-2 font-mono text-xs">{entry.original}</td>
                  <td className="px-3 py-2 font-mono text-xs">{entry.replacement}</td>
                  <td className="px-3 py-2 text-xs">{entry.language}</td>
                  <td className="px-3 py-2">
                    <span className={`text-xs px-1.5 py-0.5 rounded ${
                      entry.priority === 'high' ? 'bg-orange-100 text-orange-700' :
                      entry.priority === 'low' ? 'bg-gray-100 text-gray-500' :
                      'bg-blue-100 text-blue-700'
                    }`}>
                      {entry.priority}
                    </span>
                  </td>
                  <td className="px-3 py-2 text-center">
                    <button
                      onClick={() => handleToggle(entry)}
                      className={`w-8 h-4 rounded-full transition-colors relative ${
                        entry.enabled !== 0 ? 'bg-primary' : 'bg-muted-foreground/30'
                      }`}
                    >
                      <span className={`absolute top-0.5 w-3 h-3 rounded-full bg-white transition-transform ${
                        entry.enabled !== 0 ? 'translate-x-4' : 'translate-x-0.5'
                      }`} />
                    </button>
                  </td>
                  <td className="px-3 py-2 text-right">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        onClick={() => handleOpenEdit(entry)}
                        className="p-1 rounded hover:bg-muted transition-colors"
                      >
                        <Pencil className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={() => handleDelete(entry.id)}
                        className="p-1 rounded hover:bg-destructive/10 text-destructive transition-colors"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </td>
                </motion.tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Edit Dialog (inline modal) */}
      {editDialog.open && editDialog.entry && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
             onClick={() => setEditDialog({ open: false, entry: null })}>
          <div className="bg-background border rounded-xl shadow-2xl p-6 w-full max-w-md space-y-4"
               onClick={e => e.stopPropagation()}>
            <h3 className="text-lg font-semibold">
              {editDialog.entry.id ? 'Edit Term' : 'New Term'}
            </h3>

            <div className="space-y-3">
              <div>
                <label className="text-xs font-medium block mb-1">Original *</label>
                <input
                  type="text"
                  value={editDialog.entry.original || ''}
                  onChange={e => setEditDialog(prev => ({
                    ...prev,
                    entry: { ...prev.entry!, original: e.target.value }
                  }))}
                  className="w-full border rounded-md px-3 py-1.5 text-sm font-mono"
                  placeholder="e.g. ポリウレタン"
                />
              </div>

              <div>
                <label className="text-xs font-medium block mb-1">Replacement *</label>
                <input
                  type="text"
                  value={editDialog.entry.replacement || ''}
                  onChange={e => setEditDialog(prev => ({
                    ...prev,
                    entry: { ...prev.entry!, replacement: e.target.value }
                  }))}
                  className="w-full border rounded-md px-3 py-1.5 text-sm font-mono"
                  placeholder="e.g. polyurethane"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-xs font-medium block mb-1">Language</label>
                  <select
                    value={editDialog.entry.language || 'auto'}
                    onChange={e => setEditDialog(prev => ({
                      ...prev,
                      entry: { ...prev.entry!, language: e.target.value }
                    }))}
                    className="w-full border rounded-md px-2 py-1.5 text-sm"
                  >
                    {LANGUAGES.map(l => (
                      <option key={l} value={l}>{l}</option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="text-xs font-medium block mb-1">Priority</label>
                  <select
                    value={editDialog.entry.priority || 'normal'}
                    onChange={e => setEditDialog(prev => ({
                      ...prev,
                      entry: { ...prev.entry!, priority: e.target.value }
                    }))}
                    className="w-full border rounded-md px-2 py-1.5 text-sm"
                  >
                    {PRIORITIES.map(p => (
                      <option key={p} value={p}>{p}</option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="text-xs font-medium block mb-1">Category</label>
                  <select
                    value={editDialog.entry.category || 'general'}
                    onChange={e => setEditDialog(prev => ({
                      ...prev,
                      entry: { ...prev.entry!, category: e.target.value }
                    }))}
                    className="w-full border rounded-md px-2 py-1.5 text-sm"
                  >
                    {CATEGORIES.map(c => (
                      <option key={c} value={c}>{c}</option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="flex items-center gap-4">
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={editDialog.entry.whole_word !== false}
                    onChange={e => setEditDialog(prev => ({
                      ...prev,
                      entry: { ...prev.entry!, whole_word: e.target.checked }
                    }))}
                    className="rounded"
                  />
                  Whole word
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={editDialog.entry.case_sensitive === true}
                    onChange={e => setEditDialog(prev => ({
                      ...prev,
                      entry: { ...prev.entry!, case_sensitive: e.target.checked }
                    }))}
                    className="rounded"
                  />
                  Case sensitive
                </label>
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => setEditDialog({ open: false, entry: null })}
                className="px-4 py-1.5 text-sm rounded-md border hover:bg-muted transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                className="px-4 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
