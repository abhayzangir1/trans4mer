import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { 
  Bot, 
  Activity, 
  Square, 
  RefreshCw, 
  Users, 
  Cpu, 
  MessageSquare, 
  Search, 
  ShieldCheck, 
  CheckCircle2, 
  ExternalLink 
} from 'lucide-react';
import { useUiStore } from '../../store/uiStore';
import { useConversationStore } from '../../store/conversationStore';
import { useSwarmStore } from '../../store/swarmStore';

interface FleetAgent {
  id: string;
  projectId: string;
  name: string;
  role: string;
  definitionId: string;
  status: string;
  model?: string;
  capabilities?: string[];
  currentStep?: number;
  maxSteps?: number;
  executionId?: string;
}

export default function FleetDashboard() {
  const { activeProjectId, setActiveTab } = useUiStore();
  const { setActiveDmAgent } = useConversationStore();
  const { setSelectedAgent } = useSwarmStore();

  const [agents, setAgents] = useState<FleetAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [killingId, setKillingId] = useState<string | null>(null);
  const [substrateStatus, setSubstrateStatus] = useState<any>(null);

  // Fetch agents and their active executions
  const refreshFleet = async () => {
    if (!activeProjectId) return;
    setLoading(true);
    try {
      const rawAgents = await invoke<any[]>('list_agents', { projectId: activeProjectId });
      const activeExecs = await invoke<any[]>('list_active_executions', { projectId: activeProjectId }).catch(() => []);
      const sysStatus = await invoke<any>('cmd_get_system_status').catch(() => null);
      if (sysStatus) setSubstrateStatus(sysStatus);

      const execMap = new Map<string, any>();
      for (const ex of activeExecs) {
        execMap.set(ex.agent_instance_id, ex);
      }

      const mapped: FleetAgent[] = (rawAgents || []).map((a: any) => {
        const ex = execMap.get(a.id);
        return {
          id: a.id,
          projectId: a.project_id || activeProjectId,
          name: a.name || a.definition_id || 'Agent',
          role: a.role || 'Specialist',
          definitionId: a.definition_id,
          status: ex ? ex.status : (a.status || 'Idle'),
          model: a.model || 'Default',
          capabilities: Array.isArray(a.capabilities) ? a.capabilities : [],
          currentStep: ex ? ex.current_step : 0,
          maxSteps: ex ? ex.max_steps : 25,
          executionId: ex ? ex.id : undefined,
        };
      });

      setAgents(mapped);
    } catch (err) {
      console.error('Failed to load fleet data:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refreshFleet();
    const unlisten = listen('domain_event', () => {
      refreshFleet();
    });

    const interval = setInterval(refreshFleet, 5000);

    return () => {
      unlisten.then((f) => f());
      clearInterval(interval);
    };
  }, [activeProjectId]);

  const handleKillExecution = async (agent: FleetAgent) => {
    if (!activeProjectId || !agent.executionId) return;
    setKillingId(agent.id);
    try {
      await invoke('kill_agent_execution', {
        projectId: activeProjectId,
        executionId: agent.executionId,
      });
      await refreshFleet();
    } catch (err) {
      console.error('Failed to terminate execution:', err);
    } finally {
      setKillingId(null);
    }
  };

  const handleOpenDm = (agent: FleetAgent) => {
    setActiveDmAgent({
      id: agent.id,
      name: agent.name,
      role: agent.role,
      status: agent.status,
      definition_id: agent.definitionId,
    });
    setActiveTab('chat');
  };

  const handleInspect = (agent: FleetAgent) => {
    setSelectedAgent(agent.id);
    setActiveTab('swarm');
  };

  // KPIs
  const totalAgents = agents.length;
  const runningAgents = agents.filter((a) => a.status === 'Running' || a.status === 'Working').length;
  const queuedAgents = agents.filter((a) => a.status === 'Queued').length;
  const idleAgents = agents.filter((a) => a.status === 'Idle').length;

  const filteredAgents = agents.filter((a) => {
    const matchesStatus =
      filterStatus === 'all' ||
      (filterStatus === 'running' && (a.status === 'Running' || a.status === 'Working')) ||
      (filterStatus === 'idle' && a.status === 'Idle') ||
      (filterStatus === 'failed' && a.status === 'Failed');

    const matchesSearch =
      a.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      a.role.toLowerCase().includes(searchQuery.toLowerCase()) ||
      a.id.toLowerCase().includes(searchQuery.toLowerCase());

    return matchesStatus && matchesSearch;
  });

  return (
    <div className="h-full flex flex-col bg-bg-base overflow-y-auto p-6 font-mono select-none">
      {/* Top Banner & Title */}
      <div className="flex items-center justify-between pb-6 border-b border-gray-800">
        <div className="flex items-center space-x-3">
          <div className="p-2.5 rounded-lg bg-brand-primary/10 text-brand-primary border border-brand-primary/20">
            <Cpu size={22} />
          </div>
          <div>
            <h1 className="text-xl font-bold text-gray-100 flex items-center space-x-2">
              <span>Sovereign Fleet Dashboard</span>
              <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-normal">
                CRASH-SAFE RESUMPTION
              </span>
            </h1>
            <p className="text-xs text-gray-500 mt-0.5">
              Live daemon supervision, step-level token tracking, and instantaneous execution control.
            </p>
          </div>
        </div>

        <div className="flex items-center space-x-2">
          <button
            onClick={() => setActiveTab('swarm')}
            className="px-3 py-1.5 text-xs rounded bg-gray-800 hover:bg-gray-700 text-gray-200 flex items-center space-x-1.5 border border-gray-700 transition-colors"
          >
            <Users size={13} />
            <span>Swarm Topology</span>
          </button>
          <button
            onClick={refreshFleet}
            disabled={loading}
            className="px-3 py-1.5 text-xs rounded bg-brand-primary/20 hover:bg-brand-primary/30 text-brand-primary flex items-center space-x-1.5 border border-brand-primary/30 transition-colors disabled:opacity-50"
          >
            <RefreshCw size={13} className={loading ? 'animate-spin' : ''} />
            <span>Sync Fleet</span>
          </button>
        </div>
      </div>

      {/* KPI Cards */}
      <div className="grid grid-cols-4 gap-4 my-6">
        <div className="p-4 rounded-lg bg-bg-surface border border-gray-800 flex items-center justify-between">
          <div>
            <div className="text-[11px] text-gray-500 uppercase tracking-wider font-semibold">Total Fleet Size</div>
            <div className="text-2xl font-bold text-gray-100 mt-1">{totalAgents}</div>
            <div className="text-[10px] text-gray-400 mt-1">Autonomous Daemons</div>
          </div>
          <div className="p-2.5 rounded-lg bg-blue-500/10 text-blue-400 border border-blue-500/20">
            <Bot size={20} />
          </div>
        </div>

        <div className="p-4 rounded-lg bg-bg-surface border border-gray-800 flex items-center justify-between">
          <div>
            <div className="text-[11px] text-gray-500 uppercase tracking-wider font-semibold">Active Executions</div>
            <div className="text-2xl font-bold text-amber-400 mt-1 flex items-center space-x-2">
              <span>{runningAgents}</span>
              {runningAgents > 0 && <span className="w-2.5 h-2.5 rounded-full bg-amber-400 animate-ping" />}
            </div>
            <div className="text-[10px] text-gray-400 mt-1">{queuedAgents} queued in buffer</div>
          </div>
          <div className="p-2.5 rounded-lg bg-amber-500/10 text-amber-400 border border-amber-500/20">
            <Activity size={20} />
          </div>
        </div>

        <div className="p-4 rounded-lg bg-bg-surface border border-gray-800 flex items-center justify-between">
          <div>
            <div className="text-[11px] text-gray-500 uppercase tracking-wider font-semibold">Idle Daemons</div>
            <div className="text-2xl font-bold text-emerald-400 mt-1">{idleAgents}</div>
            <div className="text-[10px] text-gray-400 mt-1">Ready for dispatch</div>
          </div>
          <div className="p-2.5 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
            <CheckCircle2 size={20} />
          </div>
        </div>

        <div className="p-4 rounded-lg bg-bg-surface border border-gray-800 flex items-center justify-between">
          <div>
            <div className="text-[11px] text-gray-500 uppercase tracking-wider font-semibold">Substrate Health</div>
            <div className="text-2xl font-bold text-emerald-400 mt-1">
              {substrateStatus ? (substrateStatus.database_status === 'Connected' ? 'Online' : 'Degraded') : 'Sovereign'}
            </div>
            <div className="text-[10px] text-gray-400 mt-1 truncate">
              DB: {substrateStatus?.database_status || 'Checking'} • Ollama: {substrateStatus?.ollama_status || 'Probing'}
            </div>
          </div>
          <div className="p-2.5 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
            <ShieldCheck size={20} />
          </div>
        </div>
      </div>

      {/* Filter and Search Bar */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center space-x-2">
          <div className="relative">
            <Search size={14} className="absolute left-3 top-2.5 text-gray-500" />
            <input
              type="text"
              placeholder="Search by agent name, role, or ID..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9 pr-3 py-1.5 text-xs bg-bg-surface border border-gray-800 rounded focus:border-brand-primary focus:outline-none w-72 text-gray-200 placeholder-gray-600"
            />
          </div>

          <div className="flex items-center space-x-1 border border-gray-800 rounded p-0.5 bg-bg-surface">
            {['all', 'running', 'idle', 'failed'].map((st) => (
              <button
                key={st}
                onClick={() => setFilterStatus(st)}
                className={`px-2.5 py-1 text-xs uppercase rounded transition-colors ${
                  filterStatus === st ? 'bg-gray-800 text-brand-primary font-semibold' : 'text-gray-400 hover:text-gray-200'
                }`}
              >
                {st}
              </button>
            ))}
          </div>
        </div>

        <div className="text-xs text-gray-500">
          Showing <span className="text-gray-200 font-semibold">{filteredAgents.length}</span> of {totalAgents} daemons
        </div>
      </div>

      {/* Agent Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredAgents.map((agent) => {
          const isRunning = agent.status === 'Running' || agent.status === 'Working';
          const isKilling = killingId === agent.id;

          return (
            <div
              key={agent.id}
              className="p-4 rounded-lg bg-bg-surface border border-gray-800 hover:border-gray-700 transition-colors flex flex-col justify-between"
            >
              <div>
                {/* Agent Header */}
                <div className="flex items-start justify-between">
                  <div className="flex items-center space-x-2.5">
                    <div className="p-2 rounded bg-gray-800/80 text-gray-300 border border-gray-700/50">
                      <Bot size={18} />
                    </div>
                    <div>
                      <div className="font-bold text-sm text-gray-100 flex items-center space-x-1.5">
                        <span>{agent.name}</span>
                        <span
                          className={`w-2 h-2 rounded-full ${
                            isRunning
                              ? 'bg-amber-400 animate-ping'
                              : agent.status === 'Idle'
                              ? 'bg-emerald-400'
                              : 'bg-rose-500'
                          }`}
                        />
                      </div>
                      <div className="text-xs text-gray-400">{agent.role}</div>
                    </div>
                  </div>

                  <span
                    className={`text-[10px] px-2 py-0.5 rounded font-semibold uppercase tracking-wider ${
                      isRunning
                        ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                        : agent.status === 'Idle'
                        ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                        : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                    }`}
                  >
                    {agent.status}
                  </span>
                </div>

                {/* Sub-details */}
                <div className="mt-3 space-y-1.5 text-xs text-gray-400">
                  <div className="flex items-center justify-between">
                    <span className="text-gray-500">ID:</span>
                    <span className="font-mono text-[11px] text-gray-300">{agent.id.slice(0, 16)}...</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-gray-500">Model:</span>
                    <span className="text-gray-300">{agent.model}</span>
                  </div>
                </div>

                {/* Progress Bar for Active Execution */}
                {isRunning && (
                  <div className="mt-3 p-2 rounded bg-black/30 border border-gray-800">
                    <div className="flex items-center justify-between text-[10px] text-gray-400 mb-1">
                      <span>ReAct Step Progress</span>
                      <span className="font-semibold text-amber-400">
                        {agent.currentStep} / {agent.maxSteps}
                      </span>
                    </div>
                    <div className="w-full h-1.5 bg-gray-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-amber-400 rounded-full transition-all duration-300"
                        style={{
                          width: `${Math.min(100, ((agent.currentStep || 1) / (agent.maxSteps || 25)) * 100)}%`,
                        }}
                      />
                    </div>
                  </div>
                )}
              </div>

              {/* Action Buttons */}
              <div className="mt-4 pt-3 border-t border-gray-800/80 flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <button
                    onClick={() => handleOpenDm(agent)}
                    className="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 border border-gray-700/50"
                    title="Direct Message Agent"
                  >
                    <MessageSquare size={13} />
                  </button>
                  <button
                    onClick={() => handleInspect(agent)}
                    className="p-1.5 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 border border-gray-700/50"
                    title="View in Swarm Topology"
                  >
                    <ExternalLink size={13} />
                  </button>
                </div>

                {isRunning && agent.executionId && (
                  <button
                    onClick={() => handleKillExecution(agent)}
                    disabled={isKilling}
                    className="px-2.5 py-1 text-xs rounded bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/30 flex items-center space-x-1.5 transition-colors disabled:opacity-50"
                  >
                    <Square size={12} className={isKilling ? 'animate-spin' : ''} />
                    <span>{isKilling ? 'Terminating...' : 'Kill Execution'}</span>
                  </button>
                )}
              </div>
            </div>
          );
        })}

        {filteredAgents.length === 0 && (
          <div className="col-span-full py-12 text-center text-gray-500 border border-dashed border-gray-800 rounded-lg">
            <Bot size={36} className="mx-auto mb-2 opacity-40" />
            <p className="text-sm font-semibold text-gray-400">No agents match your filter criteria.</p>
            <p className="text-xs text-gray-600 mt-1">Spawn agents in the Swarm tab or send a prompt to dispatch a task.</p>
          </div>
        )}
      </div>
    </div>
  );
}
