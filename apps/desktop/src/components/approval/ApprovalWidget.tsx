import { useEffect, useState } from 'react';
import { ShieldAlert, Check, X, FileText } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../store/uiStore';
import DiffReviewPanel, { ActionDiff } from './DiffReviewPanel';

interface ApprovalRequest {
  id: string;
  agent: string;
  action: string;
  target: string;
  rawArgs?: any;
}

function formatActionSummary(action: string, rawArgs: any): { title: string; detail: string } {
  let parsed: any = null;
  if (typeof rawArgs === 'string') {
    try {
      parsed = JSON.parse(rawArgs);
    } catch {
      parsed = rawArgs;
    }
  } else {
    parsed = rawArgs;
  }

  if (typeof parsed !== 'object' || !parsed) {
    return { title: action, detail: String(rawArgs || 'No details provided') };
  }

  switch (action) {
    case 'filesystem.write':
      return {
        title: `Modify file: ${parsed.path || 'Unknown path'}`,
        detail: parsed.content ? (parsed.content.length > 200 ? parsed.content.slice(0, 200) + '...' : parsed.content) : 'Updating file contents'
      };
    case 'filesystem.delete':
      return {
        title: `Delete file: ${parsed.path || 'Unknown path'}`,
        detail: 'Permanent deletion of workspace file'
      };
    case 'terminal.exec':
      return {
        title: `Execute Shell Command`,
        detail: `$ ${parsed.command || 'Unknown command'}`
      };
    case 'delegate_task':
      return {
        title: `Delegate to Specialist (${parsed.agent_definition_id || 'Worker'})`,
        detail: parsed.instructions || 'Autonomous sub-task delegation'
      };
    case 'message.send':
      return {
        title: `Send Message to #${parsed.channel_id || 'general'}`,
        detail: parsed.content || ''
      };
    default:
      return {
        title: action,
        detail: JSON.stringify(parsed, null, 2)
      };
  }
}

export default function ApprovalWidget() {
  const { activeProjectId, showToast } = useUiStore();
  const [pendingApprovals, setPendingApprovals] = useState<ApprovalRequest[]>([]);
  const [activeDiff, setActiveDiff] = useState<ActionDiff | null>(null);
  const [loadingDiffId, setLoadingDiffId] = useState<string | null>(null);
  const [agentNameMap, setAgentNameMap] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!activeProjectId) return;
    invoke<any[]>('list_agents', { projectId: activeProjectId })
      .then((agents) => {
        if (Array.isArray(agents)) {
          const map: Record<string, string> = {};
          agents.forEach(a => {
            const name = a.name || a.definition_id || 'Agent';
            map[a.id] = name;
            if (a.definition_id) map[a.definition_id] = name;
          });
          setAgentNameMap(map);
        }
      })
      .catch(() => {});
  }, [activeProjectId]);

  const fetchApprovals = async () => {
    if (!activeProjectId) return;
    try {
      const approvals = await invoke<any[]>('get_pending_approvals', { projectId: activeProjectId });
      if (Array.isArray(approvals)) {
        setPendingApprovals(approvals.map(a => ({
          id: a.id,
          agent: a.agent_instance_id || a.agent_id || 'Agent',
          action: a.tool_name,
          target: typeof a.arguments_summary === 'string' ? a.arguments_summary : JSON.stringify(a.arguments_summary || a.tool_args || 'Unknown'),
          rawArgs: a.arguments_summary || a.tool_args
        })));
      }
    } catch (err) {
      console.error('Failed to load pending approvals:', err);
    }
  };

  useEffect(() => {
    if (!activeProjectId) return;

    fetchApprovals();

    // Real native Tauri IPC event listener for approvals
    const unlisten = listen<string>('domain_event', (event) => {
      try {
        const parsed = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload;
        const ev = parsed.event || parsed;
        const eventType = ev.type || ev.event_type;
        const data = ev.data || ev.payload || {};

        if (eventType === 'PendingApproval' || eventType === 'ApprovalRequested') {
          const reqId = data.approval_id || data.id || Date.now().toString();
          setPendingApprovals(prev => {
            if (prev.find(r => r.id === reqId)) return prev;
            return [...prev, {
              id: reqId,
              agent: data.agent_instance_id || data.agent_id || 'Agent',
              action: data.tool_name || 'Execute',
              target: typeof data.arguments_summary === 'string' ? data.arguments_summary : JSON.stringify(data.arguments_summary || data.arguments || data.payload || data.tool_args || 'Unknown target'),
              rawArgs: data.arguments_summary || data.arguments || data.payload || data.tool_args
            }];
          });
        } else if (eventType === 'ApprovalResolved') {
          const resolvedId = data.approval_id || data.id;
          setPendingApprovals(prev => prev.filter(req => req.id !== resolvedId));
          if (activeDiff && activeDiff.approval_id === resolvedId) {
            setActiveDiff(null);
          }
        }
      } catch (err) {
        console.error("Failed to parse approval event", err);
      }
    });

    const onFocus = () => fetchApprovals();
    window.addEventListener('focus', onFocus);

    return () => {
      unlisten.then(f => f());
      window.removeEventListener('focus', onFocus);
    };
  }, [activeProjectId, activeDiff]);

  const inspectDiff = async (approvalId: string) => {
    if (!activeProjectId) return;
    setLoadingDiffId(approvalId);
    try {
      const diff = await invoke<ActionDiff | null>('get_action_diff_by_approval', {
        projectId: activeProjectId,
        approvalId,
      });
      if (diff) {
        setActiveDiff(diff);
      } else {
        // No structured diff found, fall back to basic approval view
        console.log("No structured diff attached to approval " + approvalId);
      }
    } catch (e: any) {
      console.error("Failed to inspect diff for approval " + approvalId, e);
      showToast('error', typeof e === 'string' ? e : e?.message || 'Failed to inspect diff');
    } finally {
      setLoadingDiffId(null);
    }
  };

  const handleResolution = async (id: string, approved: boolean, selectedHunkIds?: string[]) => {
    try {
      if (activeDiff && activeDiff.approval_id === id) {
        // Resolve the diff first with hunk updates
        const updatedHunks = activeDiff.hunks.map(h => ({
          ...h,
          approved: selectedHunkIds ? selectedHunkIds.includes(h.id) : approved,
        }));

        await invoke('resolve_action_diff', {
          projectId: activeProjectId,
          diffId: activeDiff.id,
          decision: approved ? 'Approved' : 'Rejected',
          hunks: updatedHunks,
        });
        setActiveDiff(null);
      }

      // Optimistically filter
      setPendingApprovals(prev => prev.filter(req => req.id !== id));
      // Send resolution back to the backend
      await invoke('resolve_approval', { projectId: activeProjectId, approvalId: id, approved, feedback: null });
      showToast('info', approved ? 'Action approved' : 'Action rejected');
    } catch (e: any) {
      console.error("Failed to resolve approval, resyncing from DB:", e);
      showToast('error', typeof e === 'string' ? e : e?.message || 'Failed to resolve approval');
      fetchApprovals();
    }
  };

  if (pendingApprovals.length === 0 && !activeDiff) return null;

  return (
    <>
      {activeDiff && (
        <DiffReviewPanel
          diff={activeDiff}
          onApprove={(selectedHunkIds) => {
            if (activeDiff.approval_id) {
              handleResolution(activeDiff.approval_id, true, selectedHunkIds);
            }
          }}
          onReject={() => {
            if (activeDiff.approval_id) {
              handleResolution(activeDiff.approval_id, false);
            }
          }}
          onClose={() => setActiveDiff(null)}
        />
      )}

      {pendingApprovals.length > 0 && (
        <div className="absolute top-4 right-4 w-80 bg-red-950/80 border border-red-500/50 rounded-lg p-4 shadow-2xl backdrop-blur-md z-40">
          <div className="flex items-center space-x-2 text-red-400 font-bold text-xs uppercase mb-3">
            <ShieldAlert size={16} />
            <span>Pending Approval ({pendingApprovals.length})</span>
          </div>
          
          {pendingApprovals.map(req => {
            const agentDisplayName = agentNameMap[req.agent] || (req.agent.length > 8 ? `${req.agent.slice(0, 8)}...` : req.agent);
            const summary = formatActionSummary(req.action, req.rawArgs || req.target);
            return (
              <div key={req.id} className="bg-black/40 p-3 rounded mb-3 border border-red-500/20">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-semibold text-gray-200">@{agentDisplayName}</span>
                  <button
                    onClick={() => inspectDiff(req.id)}
                    disabled={loadingDiffId === req.id}
                    className="text-[11px] text-cyan-400 hover:text-cyan-300 flex items-center space-x-1 hover:underline"
                  >
                    <FileText size={12} />
                    <span>{loadingDiffId === req.id ? 'Loading...' : 'Inspect Diff'}</span>
                  </button>
                </div>
                <div className="text-xs text-amber-300 font-medium mt-1">
                  {summary.title}
                </div>
                <div className="font-mono text-[11px] bg-black/60 p-2 mt-2 rounded text-gray-300 break-all max-h-24 overflow-y-auto whitespace-pre-wrap">
                  {summary.detail}
                </div>
                <div className="flex space-x-2 mt-3">
                  <button 
                    onClick={() => handleResolution(req.id, true)}
                    className="flex-1 bg-emerald-600/30 hover:bg-emerald-600/50 border border-emerald-500/40 text-emerald-300 py-1.5 rounded text-xs font-semibold flex items-center justify-center space-x-1 transition-colors"
                  >
                    <Check size={14} />
                    <span>Approve</span>
                  </button>
                  <button 
                    onClick={() => handleResolution(req.id, false)}
                    className="flex-1 bg-red-500/20 hover:bg-red-500/30 border border-red-500/30 text-red-400 py-1.5 rounded text-xs font-semibold flex items-center justify-center space-x-1 transition-colors"
                  >
                    <X size={14} />
                    <span>Reject</span>
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
