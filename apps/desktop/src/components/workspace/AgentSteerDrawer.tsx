import { useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { 
  X, 
  Bot, 
  Play, 
  Pause, 
  Square, 
  Send, 
  MessageSquare, 
  Wrench, 
  AlertCircle, 
  CheckCircle2, 
  Clock, 
  Terminal,
  Activity,
  ChevronDown,
  ChevronRight
} from 'lucide-react';
import { useConversationStore } from '../../store/conversationStore';
import { useUiStore } from '../../store/uiStore';

export interface AgentSteerDrawerProps {
  projectId: string;
  agent: {
    id: string;
    name?: string;
    role?: string;
    definitionId?: string;
    definition_id?: string;
    status: string;
    model?: string;
    depthLevel?: number;
    depth_level?: number;
    parentId?: string | null;
    parent_instance_id?: string | null;
  } | null;
  onClose: () => void;
}

interface ReActStep {
  step_index: number;
  thought: string;
  action_intent?: any;
  result_payload?: any;
  error?: {
    error_type: string;
    message: string;
    stack_trace?: string;
  };
  created_at: string;
  completed_at?: string;
}

interface AgentExecution {
  id: string;
  agent_instance_id: string;
  conversation_id: string;
  status: string;
  generation: number;
  current_step: number;
  max_steps: number;
  started_at?: string;
  updated_at: string;
  completed_at?: string;
}

interface ExecutionDetailsResponse {
  execution: AgentExecution | null;
  steps: ReActStep[];
}

export default function AgentSteerDrawer({ projectId, agent, onClose }: AgentSteerDrawerProps) {
  const { setActiveDmAgent } = useConversationStore();
  const showToast = useUiStore(state => state.showToast);
  const { setActiveTab } = useUiStore();

  const [details, setDetails] = useState<ExecutionDetailsResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [actionLoading, setActionLoading] = useState<boolean>(false);
  const [directive, setDirective] = useState<string>('');
  const [isSendingDirective, setIsSendingDirective] = useState<boolean>(false);
  const [expandedSteps, setExpandedSteps] = useState<Record<string, boolean>>({});

  const stepsEndRef = useRef<HTMLDivElement>(null);

  const displayName = agent?.name || (agent?.role ? agent.role.toUpperCase() : 'Agent');
  const displayRole = agent?.role || 'Autonomous Specialist';
  const defId = agent?.definitionId || agent?.definition_id || agent?.id;

  const fetchExecutionDetails = async () => {
    if (!agent || !projectId) return;
    try {
      const res = await invoke<ExecutionDetailsResponse>('get_agent_execution_details', {
        projectId,
        agentId: agent.id,
      });
      setDetails(res);
    } catch (err: any) {
      console.error("Failed to fetch execution details:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchExecutionDetails();
    const interval = setInterval(fetchExecutionDetails, 2000);
    return () => clearInterval(interval);
  }, [projectId, agent?.id]);

  useEffect(() => {
    let unlistenFn: (() => void) | null = null;
    listen('domain_event', (event: any) => {
      try {
        const envelope = typeof event.payload === 'string' ? JSON.parse(event.payload) : event.payload;
        const eventType = envelope?.event?.type || envelope?.event_type || envelope?.type;
        if (eventType?.startsWith('Execution') || eventType?.startsWith('Step')) {
          fetchExecutionDetails();
        }
      } catch (err) {
        console.warn('Failed to parse domain_event in AgentSteerDrawer:', err);
      }
    }).then(unlisten => { unlistenFn = unlisten; });

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, [projectId, agent?.id]);

  const handleInjectDirective = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!directive.trim() || !agent || !projectId) return;

    setIsSendingDirective(true);
    try {
      await invoke('send_message', {
        projectId,
        conversationId: projectId,
        channelId: agent.id,
        content: `[Human Advisory Directive]: ${directive.trim()}`,
        mentions: [],
      });
      setDirective('');
      showToast('success', `Directive delivered to @${displayName}`);
      setTimeout(fetchExecutionDetails, 800);
    } catch (err: any) {
      console.error("Failed to inject directive:", err);
      showToast('error', `Directive injection error: ${err?.message || err}`);
    } finally {
      setIsSendingDirective(false);
    }
  };

  const handlePause = async () => {
    if (!agent || !projectId) return;
    setActionLoading(true);
    try {
      await invoke('pause_agent', {
        projectId,
        agentInstanceId: agent.id,
      });
      showToast('info', `Paused @${displayName}`);
      await fetchExecutionDetails();
    } catch (err: any) {
      console.error("Failed to pause agent:", err);
      showToast('error', `Failed to pause agent: ${err?.message || err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleResume = async () => {
    if (!agent || !projectId) return;
    setActionLoading(true);
    try {
      await invoke('resume_agent', {
        projectId,
        agentInstanceId: agent.id,
      });
      showToast('success', `Resumed @${displayName}`);
      await fetchExecutionDetails();
    } catch (err: any) {
      console.error("Failed to resume agent:", err);
      showToast('error', `Failed to resume agent: ${err?.message || err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleCancel = async () => {
    if (!details?.execution?.id) return;
    if (!confirm("Are you sure you want to cancel this agent's active execution loop?")) return;
    setActionLoading(true);
    try {
      await invoke('cancel_execution', {
        executionId: details.execution.id,
      });
      showToast('warning', `Cancelled execution for @${displayName}`);
      await fetchExecutionDetails();
    } catch (err: any) {
      console.error("Failed to cancel execution:", err);
      showToast('error', `Failed to cancel execution: ${err?.message || err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleOpenDm = () => {
    if (!agent) return;
    setActiveDmAgent({
      id: agent.id,
      name: displayName,
      role: displayRole,
      definition_id: defId || agent.id,
      status: agent.status || 'Idle',
    });
    setActiveTab('chat');
    onClose();
  };

  const toggleStep = (idx: number) => {
    setExpandedSteps(prev => ({ ...prev, [idx]: !prev[idx] }));
  };

  if (!agent) return null;

  const execution = details?.execution;
  const steps = details?.steps || [];
  const isWorking = agent.status === 'Running' || agent.status === 'Working';
  const isPaused = agent.status === 'Paused';

  return (
    <div className="fixed inset-y-0 right-0 z-40 w-[480px] bg-bg-surface border-l border-gray-800 shadow-2xl flex flex-col font-mono text-xs select-none">
      {/* Header */}
      <div className="p-4 border-b border-gray-800 flex items-center justify-between bg-bg-base">
        <div className="flex items-center space-x-2.5 truncate">
          <div className="w-7 h-7 rounded bg-brand-primary/10 border border-brand-primary/30 flex items-center justify-center text-brand-primary shrink-0">
            <Bot size={16} />
          </div>
          <div className="truncate">
            <div className="flex items-center space-x-1.5 font-bold text-gray-100">
              <span className="truncate">@{displayName}</span>
              <span className="text-[10px] text-gray-500 font-normal">[{defId}]</span>
            </div>
            <div className="text-[10px] text-gray-400 truncate">{displayRole}</div>
          </div>
        </div>

        <div className="flex items-center space-x-1.5">
          <button
            onClick={handleOpenDm}
            className="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-brand-primary transition-colors"
            title="Open 1:1 Direct Message"
          >
            <MessageSquare size={14} />
          </button>
          <button
            onClick={onClose}
            className="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
            title="Close Drawer"
          >
            <X size={15} />
          </button>
        </div>
      </div>

      {/* Main Drawer Body */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {/* Status & Control Strip */}
        <div className="bg-bg-base rounded-lg border border-gray-800 p-3 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <span className={`h-2.5 w-2.5 rounded-full ${
                isWorking ? 'bg-yellow-400 animate-ping' : isPaused ? 'bg-gray-500' : 'bg-green-400'
              }`} />
              <span className="font-bold text-gray-200 uppercase tracking-wide text-[11px]">
                Status: {agent.status}
              </span>
            </div>

            <div className="flex items-center space-x-1.5">
              {isPaused ? (
                <button
                  onClick={handleResume}
                  disabled={actionLoading}
                  className="flex items-center space-x-1 px-2 py-1 rounded bg-green-900/30 text-green-400 border border-green-800/60 hover:bg-green-900/50 transition-colors disabled:opacity-50"
                  title="Resume Agent Execution"
                >
                  <Play size={11} />
                  <span>Resume</span>
                </button>
              ) : (
                <button
                  onClick={handlePause}
                  disabled={actionLoading}
                  className="flex items-center space-x-1 px-2 py-1 rounded bg-yellow-900/30 text-yellow-400 border border-yellow-800/60 hover:bg-yellow-900/50 transition-colors disabled:opacity-50"
                  title="Pause Agent Execution"
                >
                  <Pause size={11} />
                  <span>Pause</span>
                </button>
              )}

              {execution && execution.status !== 'Completed' && execution.status !== 'Failed' && (
                <button
                  onClick={handleCancel}
                  disabled={actionLoading}
                  className="flex items-center space-x-1 px-2 py-1 rounded bg-red-900/30 text-red-400 border border-red-800/60 hover:bg-red-900/50 transition-colors disabled:opacity-50"
                  title="Cancel Execution"
                >
                  <Square size={11} />
                  <span>Cancel</span>
                </button>
              )}
            </div>
          </div>

          {execution ? (
            <div className="grid grid-cols-2 gap-2 pt-2 border-t border-gray-800/70 text-[10px] text-gray-400">
              <div>
                <span className="text-gray-500 block">Execution:</span>
                <span className="text-gray-300 font-semibold">{execution.status}</span>
              </div>
              <div>
                <span className="text-gray-500 block">Step Progress:</span>
                <span className="text-brand-primary font-bold">
                  {execution.current_step} / {execution.max_steps}
                </span>
              </div>
              <div>
                <span className="text-gray-500 block">Generation:</span>
                <span className="text-gray-300">v{execution.generation}</span>
              </div>
              <div>
                <span className="text-gray-500 block">Last Active:</span>
                <span className="text-gray-300">
                  {new Date(execution.updated_at).toLocaleTimeString()}
                </span>
              </div>
            </div>
          ) : (
            <div className="pt-2 border-t border-gray-800/70 text-[10px] text-gray-500 italic">
              No active execution recorded in current workspace.
            </div>
          )}
        </div>

        {/* Mid-Run Directive Box */}
        <div className="bg-bg-base rounded-lg border border-gray-800 p-3 space-y-2">
          <div className="flex items-center justify-between text-[11px] font-bold text-gray-300">
            <span className="flex items-center space-x-1.5">
              <Terminal size={13} className="text-brand-primary" />
              <span>Mid-Run Directive</span>
            </span>
            <span className="text-[10px] text-gray-500 font-normal">Injects directly into inbox</span>
          </div>

          <form onSubmit={handleInjectDirective} className="space-y-2">
            <textarea
              value={directive}
              onChange={(e) => setDirective(e.target.value)}
              placeholder={`Advisory guidance for @${displayName}... e.g., "Skip unit tests for now and focus on fixing the compiler error."`}
              rows={2}
              className="w-full bg-black/40 border border-gray-700 rounded p-2 text-xs text-gray-200 focus:outline-none focus:border-brand-primary resize-none"
            />
            <div className="flex justify-end">
              <button
                type="submit"
                disabled={isSendingDirective || !directive.trim()}
                className="flex items-center space-x-1.5 px-3 py-1.5 rounded bg-brand-primary text-black font-semibold text-xs hover:opacity-90 disabled:opacity-40 transition-opacity"
              >
                <Send size={12} />
                <span>{isSendingDirective ? 'Injecting...' : 'Send Directive'}</span>
              </button>
            </div>
          </form>
        </div>

        {/* Live ReAct Steps Tailing */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-[11px] font-bold uppercase tracking-wider text-gray-400">
            <span className="flex items-center space-x-1.5">
              <Activity size={13} className="text-brand-primary" />
              <span>Reasoning & Execution Steps ({steps.length})</span>
            </span>
            {loading && <span className="text-[10px] text-gray-500 lowercase animate-pulse">refreshing...</span>}
          </div>

          {steps.length === 0 ? (
            <div className="bg-bg-base rounded-lg border border-gray-800 p-4 text-center text-gray-500 text-[11px] italic">
              No reasoning steps recorded yet for this agent.
            </div>
          ) : (
            <div className="space-y-2 max-h-[420px] overflow-y-auto pr-1">
              {steps.map((step) => {
                const isExpanded = expandedSteps[step.step_index] ?? (step.step_index === steps.length);
                const hasError = !!step.error;
                const toolName = step.action_intent?.tool_name || step.action_intent?.name;

                return (
                  <div
                    key={step.step_index}
                    className={`rounded-lg border transition-colors ${
                      hasError
                        ? 'border-red-800/80 bg-red-950/20'
                        : 'border-gray-800 bg-bg-base'
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => toggleStep(step.step_index)}
                      className="w-full p-2.5 flex items-center justify-between text-left hover:bg-gray-800/40 rounded-t-lg transition-colors"
                    >
                      <div className="flex items-center space-x-2 truncate">
                        {isExpanded ? <ChevronDown size={13} className="text-gray-400 shrink-0" /> : <ChevronRight size={13} className="text-gray-400 shrink-0" />}
                        <span className="font-bold text-gray-300 text-[11px]">Step {step.step_index}</span>
                        {toolName && (
                          <span className="flex items-center space-x-1 px-1.5 py-0.5 rounded bg-brand-primary/15 text-brand-primary text-[10px]">
                            <Wrench size={10} />
                            <span className="truncate">{toolName}</span>
                          </span>
                        )}
                      </div>

                      <div className="flex items-center space-x-1 text-[10px] text-gray-500 shrink-0 ml-2">
                        <Clock size={10} />
                        <span>{new Date(step.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</span>
                      </div>
                    </button>

                    {isExpanded && (
                      <div className="px-3 pb-3 pt-1 space-y-2 border-t border-gray-800/50 text-[11px]">
                        {/* Thought */}
                        {step.thought && (
                          <div>
                            <span className="text-gray-500 text-[10px] uppercase font-bold block mb-0.5">Thought:</span>
                            <div className="text-gray-300 bg-black/30 rounded p-2 border border-gray-800/60 whitespace-pre-wrap leading-relaxed">
                              {step.thought}
                            </div>
                          </div>
                        )}

                        {/* Action Intent Arguments */}
                        {step.action_intent?.arguments && (
                          <div>
                            <span className="text-gray-500 text-[10px] uppercase font-bold block mb-0.5">Tool Arguments:</span>
                            <pre className="text-[10px] text-brand-primary bg-black/40 rounded p-2 border border-gray-800/60 overflow-x-auto">
                              {JSON.stringify(step.action_intent.arguments, null, 2)}
                            </pre>
                          </div>
                        )}

                        {/* Result Payload */}
                        {step.result_payload && (
                          <div>
                            <span className="text-gray-500 text-[10px] uppercase font-bold block mb-0.5 flex items-center space-x-1 text-green-400">
                              <CheckCircle2 size={11} />
                              <span>Tool Output:</span>
                            </span>
                            <div className="text-[10px] text-gray-300 bg-black/40 rounded p-2 border border-gray-800/60 overflow-x-auto max-h-36">
                              {typeof step.result_payload === 'string'
                                ? step.result_payload
                                : JSON.stringify(step.result_payload, null, 2)}
                            </div>
                          </div>
                        )}

                        {/* Error */}
                        {hasError && (
                          <div className="bg-red-900/20 border border-red-800 rounded p-2 space-y-1">
                            <div className="flex items-center space-x-1.5 text-red-400 font-bold text-[10px]">
                              <AlertCircle size={12} />
                              <span>{step.error?.error_type || 'Execution Error'}</span>
                            </div>
                            <div className="text-red-300 text-[10px]">
                              {step.error?.message}
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={stepsEndRef} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
