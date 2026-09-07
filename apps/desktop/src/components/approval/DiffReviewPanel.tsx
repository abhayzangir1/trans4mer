import { useState } from 'react';
import { Check, X, ShieldAlert, FileCode, CheckSquare, Square } from 'lucide-react';

export interface DiffHunk {
  id: string;
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  lines: string[];
  approved: boolean;
}

export interface ActionDiff {
  id: string;
  project_id: string;
  execution_id?: string;
  agent_instance_id?: string;
  capability: string;
  risk_level: string;
  kind: any;
  diff_payload: string;
  hunks: DiffHunk[];
  decision: 'Pending' | 'Approved' | 'Rejected' | 'Modified';
  force_review_reason?: string;
  approval_id?: string;
  created_at: string;
  resolved_at?: string;
}

interface DiffReviewPanelProps {
  diff: ActionDiff;
  onApprove: (selectedHunkIds: string[]) => void;
  onReject: () => void;
  onClose: () => void;
}

export default function DiffReviewPanel({
  diff,
  onApprove,
  onReject,
  onClose,
}: DiffReviewPanelProps) {
  const [selectedHunks, setSelectedHunks] = useState<Record<string, boolean>>(() => {
    const initial: Record<string, boolean> = {};
    for (const h of diff.hunks) {
      initial[h.id] = h.approved !== false;
    }
    return initial;
  });

  const toggleHunk = (id: string) => {
    setSelectedHunks(prev => ({
      ...prev,
      [id]: !prev[id],
    }));
  };

  const selectAll = () => {
    const updated: Record<string, boolean> = {};
    for (const h of diff.hunks) {
      updated[h.id] = true;
    }
    setSelectedHunks(updated);
  };

  const deselectAll = () => {
    const updated: Record<string, boolean> = {};
    for (const h of diff.hunks) {
      updated[h.id] = false;
    }
    setSelectedHunks(updated);
  };

  const handleApprove = () => {
    const approvedIds = Object.entries(selectedHunks)
      .filter(([_, checked]) => checked)
      .map(([id]) => id);
    onApprove(approvedIds);
  };

  // Extract kind details
  const isGitHubPr = diff.kind?.type === 'GitHubPrReview' || Boolean(diff.kind?.GitHubPrReview);
  const prDetails = diff.kind?.details || diff.kind?.GitHubPrReview || {};
  const kindKey = diff.kind?.type || Object.keys(diff.kind || {})[0] || 'Unknown';
  const kindVal = diff.kind?.details || (diff.kind && diff.kind[kindKey]) || {};
  const targetLabel = isGitHubPr
    ? `${prDetails.owner}/${prDetails.repo}#${prDetails.pull_number} (${prDetails.review_event || 'Review'})`
    : kindVal.path || kindVal.command || kindVal.url || kindVal.skill_name || kindKey;

  const hasSecret = Boolean(diff.force_review_reason && diff.force_review_reason.includes('Secret detected'));

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-zinc-900 border border-zinc-700 rounded-xl max-w-4xl w-full max-h-[90vh] flex flex-col shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="px-6 py-4 border-b border-zinc-800 flex items-center justify-between bg-zinc-950/60">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-amber-500/10 text-amber-400 border border-amber-500/20">
              <FileCode size={20} />
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <span className="font-semibold text-white text-base">Diff Review</span>
                <span className="text-xs px-2 py-0.5 rounded bg-zinc-800 text-zinc-300 font-mono">
                  {diff.capability}
                </span>
                <span className={`text-xs px-2 py-0.5 rounded font-bold ${
                  diff.risk_level === 'High' || diff.risk_level === 'Critical'
                    ? 'bg-red-900/50 text-red-300 border border-red-500/30'
                    : 'bg-yellow-900/50 text-yellow-300 border border-yellow-500/30'
                }`}>
                  {diff.risk_level} Risk
                </span>
              </div>
              <div className="text-xs text-zinc-400 font-mono mt-0.5">
                Target: <span className="text-zinc-200">{targetLabel}</span>
              </div>
            </div>
          </div>

          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-white p-1.5 rounded-lg hover:bg-zinc-800 transition"
          >
            <X size={18} />
          </button>
        </div>

        {/* Security Warning Banner */}
        {hasSecret && (
          <div className="bg-red-950/90 border-b border-red-700/50 px-6 py-3 flex items-start space-x-3">
            <ShieldAlert size={20} className="text-red-400 shrink-0 mt-0.5" />
            <div>
              <div className="text-sm font-bold text-red-200">Security Alert: Sensitive Data Detected</div>
              <div className="text-xs text-red-300 mt-0.5">
                {diff.force_review_reason} - Review carefully before approving.
              </div>
            </div>
          </div>
        )}

        {/* GitHub PR Review Banner */}
        {isGitHubPr && (
          <div className="bg-sky-950/60 border-b border-sky-800/40 px-6 py-2.5 flex items-center justify-between text-xs">
            <div className="flex items-center space-x-2 text-sky-200">
              <span className="font-semibold text-sky-400">GitHub PR Review:</span>
              <span>{prDetails.owner}/{prDetails.repo}#{prDetails.pull_number}</span>
              <span className="px-2 py-0.5 rounded bg-sky-900/60 text-sky-300 border border-sky-700/50 font-mono">
                {prDetails.review_event || 'COMMENT'}
              </span>
            </div>
            <span className="text-zinc-400">
              {prDetails.comments_count || diff.hunks.length} structured finding(s) with provenance
            </span>
          </div>
        )}

        {/* Diff Hunks Container */}
        <div className="flex-1 overflow-y-auto p-6 space-y-4 font-mono text-xs">
          {diff.hunks.length === 0 ? (
            <div className="bg-zinc-950/60 rounded-lg p-4 text-zinc-400">
              <pre className="whitespace-pre-wrap">{diff.diff_payload || 'No line diff available.'}</pre>
            </div>
          ) : (
            <>
              <div className="flex items-center justify-between text-xs text-zinc-400 font-sans pb-2 border-b border-zinc-800">
                <span>Select hunks to accept:</span>
                <div className="flex space-x-3">
                  <button
                    type="button"
                    onClick={selectAll}
                    className="hover:text-white underline decoration-zinc-600"
                  >
                    Select All
                  </button>
                  <button
                    type="button"
                    onClick={deselectAll}
                    className="hover:text-white underline decoration-zinc-600"
                  >
                    Deselect All
                  </button>
                </div>
              </div>

              {diff.hunks.map(hunk => {
                const isSelected = selectedHunks[hunk.id] ?? true;
                return (
                  <div
                    key={hunk.id}
                    className={`border rounded-lg overflow-hidden transition ${
                      isSelected
                        ? 'border-zinc-700 bg-zinc-950/70'
                        : 'border-zinc-800/60 bg-zinc-950/30 opacity-60'
                    }`}
                  >
                    {/* Hunk Header */}
                    <div
                      className="px-4 py-2 bg-zinc-900 border-b border-zinc-800 flex items-center justify-between cursor-pointer select-none"
                      onClick={() => toggleHunk(hunk.id)}
                    >
                      <div className="flex items-center space-x-2 text-cyan-400 font-semibold">
                        {isSelected ? (
                          <CheckSquare size={16} className="text-emerald-400" />
                        ) : (
                          <Square size={16} className="text-zinc-500" />
                        )}
                        <span>{hunk.header}</span>
                      </div>
                      <span className="text-[11px] text-zinc-500 font-sans">
                        Lines -{hunk.old_start},{hunk.old_lines} +{hunk.new_start},{hunk.new_lines}
                      </span>
                    </div>

                    {/* Hunk Lines */}
                    <div className="p-2 overflow-x-auto">
                      {hunk.lines.map((line, idx) => {
                        let lineClass = 'text-zinc-400';
                        let bgClass = '';
                        if (line.startsWith('+')) {
                          lineClass = 'text-emerald-300';
                          bgClass = 'bg-emerald-950/30';
                        } else if (line.startsWith('-')) {
                          lineClass = 'text-red-300';
                          bgClass = 'bg-red-950/30';
                        } else if (line.startsWith('// Comment:')) {
                          lineClass = 'text-amber-300 font-sans italic font-medium';
                          bgClass = 'bg-amber-950/40 border-l-2 border-amber-500 pl-2 my-1';
                        }
                        return (
                          <div
                            key={idx}
                            className={`px-2 py-0.5 rounded leading-relaxed whitespace-pre font-mono ${lineClass} ${bgClass}`}
                          >
                            {line}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
            </>
          )}
        </div>

        {/* Footer actions */}
        <div className="px-6 py-4 border-t border-zinc-800 bg-zinc-950/80 flex items-center justify-between">
          <button
            onClick={onReject}
            className="flex items-center space-x-1.5 px-4 py-2 rounded-lg bg-red-900/40 hover:bg-red-900/70 text-red-300 border border-red-700/50 font-medium text-xs transition"
          >
            <X size={16} />
            <span>Reject All Changes</span>
          </button>

          <div className="flex items-center space-x-3">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-medium text-xs transition"
            >
              Cancel
            </button>
            <button
              onClick={handleApprove}
              className="flex items-center space-x-1.5 px-5 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white font-semibold text-xs shadow-lg shadow-emerald-950/50 transition"
            >
              <Check size={16} />
              <span>Approve Selected Hunks</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
