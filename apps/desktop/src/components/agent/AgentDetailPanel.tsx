import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Activity, Cpu, Coins, Bot, ShieldCheck, FileText, Check, ListTree, Wrench, AlertCircle } from 'lucide-react';
import { useUiStore } from '../../store/uiStore';
import { useConversationStore } from '../../store/conversationStore';
import { useSwarmStore } from '../../store/swarmStore';
import { useAgents } from '../../hooks/useAgents';

interface AgentDefinition {
  id: string;
  name: string;
  role: string;
  description: string;
  system_instructions: string;
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
  created_at?: string;
  completed_at?: string;
}

export default function AgentDetailPanel() {
  const { activeProjectId } = useUiStore();
  const { activeDmAgent } = useConversationStore();
  const { selectedAgentId } = useSwarmStore();
  const { agents } = useAgents(activeProjectId);

  const [activeTab, setActiveTab] = useState<'telemetry' | 'contract'>('telemetry');
  const [tokenUsage, setTokenUsage] = useState<number>(0);
  const [loading, setLoading] = useState(false);
  const [engineStatus, setEngineStatus] = useState('Idle');
  const [executionSteps, setExecutionSteps] = useState<ReActStep[]>([]);

  // Role Contract State
  const [definition, setDefinition] = useState<AgentDefinition | null>(null);
  const [systemPrompt, setSystemPrompt] = useState('');
  const [customRole, setCustomRole] = useState('');
  const [isSavingContract, setIsSavingContract] = useState(false);
  const [contractSaved, setContractSaved] = useState(false);

  // Resolve target agent from activeDmAgent or swarm selectedAgentId
  const currentAgent = activeDmAgent || (selectedAgentId ? agents.find(a => a.id === selectedAgentId) : null);

  const defId = currentAgent
    ? ('definition_id' in currentAgent ? currentAgent.definition_id : ('definitionId' in currentAgent ? currentAgent.definitionId : ''))
    : '';

  const agentName = currentAgent 
    ? (('name' in currentAgent && currentAgent.name) ? currentAgent.name : defId)
    : null;
  
  const agentRole = currentAgent
    ? (('role' in currentAgent && currentAgent.role) ? currentAgent.role : 'Autonomous Specialist')
    : null;

  const agentStatus = currentAgent?.status || 'Idle';

  // Load Agent Definition whenever defId changes
  useEffect(() => {
    if (!defId) {
      setDefinition(null);
      setSystemPrompt('');
      setCustomRole('');
      return;
    }

    invoke<AgentDefinition>('get_agent_definition', { definitionId: defId })
      .then(def => {
        setDefinition(def);
        setSystemPrompt(def.system_instructions || '');
        setCustomRole(def.role || '');
      })
      .catch(err => {
        console.warn('Could not fetch agent definition:', err);
      });
  }, [defId]);

  useEffect(() => {
    if (!activeProjectId) return;
    
    let isMounted = true;
    setLoading(true);

    const fetchDetails = async () => {
      try {
        const agentId = currentAgent ? ('id' in currentAgent ? currentAgent.id : null) : null;
        
        // 1. Token usage
        const usage = await invoke<number>('get_token_usage', { 
          projectId: activeProjectId, 
          agentId: agentId 
        });
        if (isMounted) setTokenUsage(usage);

        // 2. Scoped execution details & reasoning steps
        if (agentId) {
          try {
            const details = await invoke<any>('get_agent_execution_details', {
              projectId: activeProjectId,
              agentId: agentId,
            });
            if (isMounted && details) {
              if (Array.isArray(details.steps)) {
                setExecutionSteps(details.steps);
              }
              if (details.execution) {
                if (details.execution.status === 'Running') {
                  setEngineStatus(`Running (Step ${details.execution.current_step}/${details.execution.max_steps})`);
                } else {
                  setEngineStatus(details.execution.status);
                }
              } else {
                setEngineStatus('Idle');
              }
            }
          } catch (err) {
            console.error("Failed to load agent execution details:", err);
          }
        } else {
          // Check global active executions strictly filtered to this agent
          const activeExecs = await invoke<any[]>('list_active_executions', { projectId: activeProjectId });
          if (isMounted) {
            const myExec = activeExecs.find(e => currentAgent && e.agent_instance_id === currentAgent.id);
            if (myExec) {
              setEngineStatus(`Running (Step ${myExec.current_step}/${myExec.max_steps})`);
            } else {
              setEngineStatus('Idle');
              setExecutionSteps([]);
            }
          }
        }
      } catch (err) {
        console.error("Failed to load agent details:", err);
      } finally {
        if (isMounted) setLoading(false);
      }
    };

    fetchDetails();
    const interval = setInterval(fetchDetails, 3000);
    return () => {
      isMounted = false;
      clearInterval(interval);
    };
  }, [activeProjectId, currentAgent]);

  const handleSaveContract = async () => {
    if (!defId || isSavingContract) return;
    setIsSavingContract(true);
    try {
      await invoke('update_agent_definition', {
        definitionId: defId,
        systemInstructions: systemPrompt,
        role: customRole.trim() ? customRole.trim() : null,
        name: null,
        description: null,
      });
      setContractSaved(true);
      setTimeout(() => setContractSaved(false), 3000);
      useUiStore.getState().showToast('success', 'Agent contract updated');
    } catch (err: any) {
      console.error('Failed to update agent role contract:', err);
      useUiStore.getState().showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to update agent role contract');
    } finally {
      setIsSavingContract(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-bg-surface text-gray-300 font-mono text-xs">
      {/* Header & Sub-Tab Switcher */}
      <div className="p-3 border-b border-gray-800 flex items-center justify-between">
        <div className="flex items-center space-x-2 font-bold uppercase tracking-wider text-gray-400">
          <Activity size={14} className="text-brand-primary" />
          <span>Agent Inspector</span>
        </div>
        <div className="flex items-center bg-gray-900 border border-gray-800 rounded p-0.5">
          <button
            onClick={() => setActiveTab('telemetry')}
            className={`px-2 py-0.5 rounded text-[10px] font-semibold transition-colors ${
              activeTab === 'telemetry'
                ? 'bg-brand-primary text-black'
                : 'text-gray-400 hover:text-gray-200'
            }`}
          >
            Telemetry
          </button>
          <button
            onClick={() => setActiveTab('contract')}
            className={`px-2 py-0.5 rounded text-[10px] font-semibold transition-colors ${
              activeTab === 'contract'
                ? 'bg-blue-600 text-white'
                : 'text-gray-400 hover:text-gray-200'
            }`}
          >
            Contract
          </button>
        </div>
      </div>
      
      <div className="flex-1 overflow-auto p-3 space-y-4">
        {/* Selected Agent Identity Card */}
        <div className="bg-bg-base p-3.5 rounded-lg border border-gray-800 space-y-2.5">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <div className={`w-7 h-7 rounded flex items-center justify-center border ${
                currentAgent 
                  ? 'bg-brand-primary/20 text-brand-primary border-brand-primary/30' 
                  : 'bg-gray-800 text-gray-500 border-gray-700'
              }`}>
                <Bot size={15} />
              </div>
              <div className="overflow-hidden">
                <h3 className="font-bold text-gray-100 text-xs truncate">
                  {agentName ? `@${agentName}` : 'No Agent Selected'}
                </h3>
                <span className="text-[10px] text-gray-400 block truncate">
                  {customRole || agentRole || 'Select an agent to inspect'}
                </span>
              </div>
            </div>
            {currentAgent ? (
              <span className={`text-[9px] px-2 py-0.5 rounded-full font-bold uppercase whitespace-nowrap ${
                agentStatus === 'Running' || agentStatus === 'Working'
                  ? 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30 animate-pulse'
                  : 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
              }`}>
                {agentStatus}
              </span>
            ) : (
              <span className="text-[9px] px-2 py-0.5 rounded-full font-bold uppercase bg-gray-800 text-gray-500 border border-gray-700">
                None
              </span>
            )}
          </div>

          <div className="pt-2 border-t border-gray-800/80 flex items-center justify-between text-[11px] text-gray-400">
            <span className="flex items-center space-x-1">
              <ShieldCheck size={12} className={currentAgent ? "text-emerald-400" : "text-gray-500"} />
              <span>Zero-Trust Sandbox</span>
            </span>
            <span className="text-[10px] text-gray-500">{currentAgent ? 'Enforced' : 'Standing By'}</span>
          </div>
        </div>

        {activeTab === 'telemetry' ? (
          <>
            {/* Execution Engine Status */}
            <div className="bg-bg-base p-3.5 rounded-lg border border-gray-800">
              <div className="flex items-center space-x-2 mb-1.5 text-brand-primary">
                <Cpu size={14} />
                <h3 className="font-semibold text-xs text-gray-300">ReAct Execution Engine</h3>
              </div>
              <div className="text-sm font-bold font-mono text-gray-100">{loading ? '...' : engineStatus}</div>
              <p className="text-[10px] text-gray-500 mt-1">Autonomous reasoning and tool dispatch loop</p>
            </div>

            {/* ReAct Execution Steps & Dispatched Tools */}
            <div className="bg-bg-base p-3.5 rounded-lg border border-gray-800 space-y-2.5">
              <div className="flex items-center justify-between text-brand-primary">
                <div className="flex items-center space-x-2">
                  <ListTree size={14} />
                  <h3 className="font-semibold text-xs text-gray-300">Reasoning & Tool Steps</h3>
                </div>
                <span className="text-[10px] text-gray-400 font-mono">
                  {executionSteps.length} step{executionSteps.length !== 1 ? 's' : ''}
                </span>
              </div>

              {executionSteps.length === 0 ? (
                <div className="py-4 text-center text-gray-500 text-[11px]">
                  <p>No active execution steps recorded for this agent.</p>
                  <p className="text-[10px] text-gray-600 mt-1">
                    Tool dispatches and inner thoughts will be tracked here live.
                  </p>
                </div>
              ) : (
                <div className="space-y-3 max-h-72 overflow-y-auto pr-1">
                  {executionSteps.map((step, idx) => {
                    const toolName = step.action_intent?.tool_name || step.action_intent?.name;
                    return (
                      <div key={idx} className="p-2.5 rounded bg-gray-950/80 border border-gray-800 space-y-1.5 text-[11px]">
                        <div className="flex items-center justify-between text-[10px] text-gray-400">
                          <span className="font-bold text-gray-300">Step #{step.step_index ?? idx + 1}</span>
                          {step.created_at && (
                            <span className="text-gray-500">{new Date(step.created_at).toLocaleTimeString()}</span>
                          )}
                        </div>

                        {step.thought && (
                          <div className="text-gray-300 font-mono leading-relaxed bg-gray-900/50 p-2 rounded border border-gray-800/50">
                            <span className="text-brand-primary/80 font-bold block text-[10px] uppercase mb-0.5">Thought</span>
                            {step.thought}
                          </div>
                        )}

                        {toolName && (
                          <div className="flex items-center space-x-1.5 text-xs text-yellow-400 bg-yellow-950/30 border border-yellow-800/40 px-2 py-1 rounded">
                            <Wrench size={11} className="shrink-0" />
                            <span className="font-bold">{toolName}</span>
                          </div>
                        )}

                        {step.error && (
                          <div className="flex items-start space-x-1.5 text-red-400 bg-red-950/30 border border-red-900/40 p-2 rounded text-[10px]">
                            <AlertCircle size={12} className="shrink-0 mt-0.5" />
                            <span>{step.error.message || step.error.error_type}</span>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Token Usage */}
            <div className="bg-bg-base p-3.5 rounded-lg border border-gray-800">
              <div className="flex items-center space-x-2 mb-1.5 text-yellow-500">
                <Coins size={14} />
                <h3 className="font-semibold text-xs text-gray-300">Token Consumption</h3>
              </div>
              <div className="text-lg font-bold font-mono text-yellow-400">{tokenUsage.toLocaleString()}</div>
              <p className="text-[10px] text-gray-500 mt-1">Total tokens consumed across executions</p>
            </div>
          </>
        ) : (
          /* Role Contract & Strict Prompt Tab */
          <div className="bg-bg-base p-3.5 rounded-lg border border-gray-800 space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-2 text-blue-400 font-semibold text-xs">
                <FileText size={14} />
                <span>Role Contract & Strict Prompt</span>
              </div>
              {contractSaved && (
                <span className="text-[10px] text-emerald-400 flex items-center space-x-1 font-medium">
                  <Check size={12} />
                  <span>Saved to SQLite</span>
                </span>
              )}
            </div>

            {definition ? (
              <>
                <div className="space-y-1.5">
                  <label className="text-[10px] text-gray-400 uppercase">Specialist Role Title</label>
                  <input
                    type="text"
                    value={customRole}
                    onChange={e => setCustomRole(e.target.value)}
                    className="w-full bg-gray-950 border border-gray-800 rounded px-2.5 py-1 text-gray-200 text-[11px] focus:outline-none focus:border-blue-500"
                  />
                </div>

                <div className="space-y-1.5">
                  <label className="text-[10px] text-gray-400 uppercase">Strict System Instructions</label>
                  <textarea
                    value={systemPrompt}
                    onChange={e => setSystemPrompt(e.target.value)}
                    rows={8}
                    className="w-full bg-gray-950 border border-gray-800 rounded p-2 text-gray-200 text-[11px] font-mono leading-relaxed resize-none focus:outline-none focus:border-blue-500"
                    placeholder="Enter strict behavioral guidelines and role boundaries..."
                  />
                </div>

                <button
                  onClick={handleSaveContract}
                  disabled={isSavingContract}
                  className="w-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white font-medium py-1.5 rounded transition-colors text-[11px]"
                >
                  {isSavingContract ? 'Saving Contract...' : 'Save Role Contract'}
                </button>
              </>
            ) : (
              <p className="text-gray-500 text-[11px] py-4 text-center">
                Select an agent to inspect and customize its role contract.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
