import { useState, useEffect } from 'react';
import { Hash, Plus, MessageSquare, FolderGit2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useProjectStore } from '../../store/projectStore';
import { useConversationStore, TeamMember } from '../../store/conversationStore';
import { useUiStore } from '../../store/uiStore';
import { useAgents } from '../../hooks/useAgents';

export default function TeamSidebar() {
  const { activeProjectId, projects } = useProjectStore();
  const { setActiveTab, showToast } = useUiStore();
  const { 
    conversations, 
    activeConversationId, 
    activeDmAgent, 
    fetchConversations, 
    createConversation, 
    setActiveConversation, 
    setActiveDmAgent,
    sidebarMode,
    setSidebarMode 
  } = useConversationStore();
  
  const { agents } = useAgents(activeProjectId);
  const [isAddChannelOpen, setIsAddChannelOpen] = useState(false);
  const [newChannelName, setNewChannelName] = useState('');
  const [teamMembers, setTeamMembers] = useState<TeamMember[]>([]);

  const activeProject = projects.find(p => p.id === activeProjectId);

  useEffect(() => {
    if (!activeProjectId) return;
    fetchConversations(activeProjectId);
  }, [activeProjectId, fetchConversations]);

  const [isSpawnAgentOpen, setIsSpawnAgentOpen] = useState(false);
  
  // Custom definition fields
  const [customName, setCustomName] = useState('');
  const [customRole, setCustomRole] = useState('');
  const [customDesc, setCustomDesc] = useState('');
  const [customPrompt, setCustomPrompt] = useState('');
  const [customModel, setCustomModel] = useState('');
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [isLoadingModels, setIsLoadingModels] = useState(false);
  const [isSubmittingAgent, setIsSubmittingAgent] = useState(false);

  // Dynamic model detection when spawn modal is opened
  useEffect(() => {
    if (!isSpawnAgentOpen) return;
    setIsLoadingModels(true);
    invoke<{ provider: string; ollama_endpoint: string }>('get_settings')
      .then((settings) => {
        const providerName = settings.provider || 'ollama';
        return invoke<string[]>('list_available_models', {
          name: providerName,
          endpoint: settings.ollama_endpoint,
        }).then((models) => {
          if (models && Array.isArray(models) && models.length > 0) {
            setAvailableModels(models);
            if (!customModel) {
              setCustomModel(models[0]);
            }
          } else {
            if (!customModel) setCustomModel('qwen2.5-coder:3b');
          }
        });
      })
      .catch((err) => {
        console.warn('Failed to detect available models:', err);
        if (!customModel) setCustomModel('qwen2.5-coder:3b');
      })
      .finally(() => {
        setIsLoadingModels(false);
      });
  }, [isSpawnAgentOpen]);

  const loadTeamMembers = () => {
    if (!activeProjectId) return;
    const isRunningInTauri = typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ !== undefined);
    if (!isRunningInTauri) {
      setTeamMembers([
        { id: 'agent-boss', name: 'Boss Agent', role: 'Chief AI Swarm Orchestrator', definition_id: 'orchestrator', status: 'Running' },
        { id: 'agent-rust-systems', name: 'Rust Systems Engineer', role: 'Tokio Concurrency & IPC', definition_id: 'rust_engineer', status: 'Working' },
        { id: 'agent-security', name: 'Security & Policy Auditor', role: 'Zero-Trust Policy Gate', definition_id: 'security_auditor', status: 'Idle' },
        { id: 'agent-memory', name: 'Memory Specialist', role: '4-Tier Cognitive RAG', definition_id: 'memory_specialist', status: 'Idle' },
        { id: 'agent-cdp', name: 'CDP Browser Navigator', role: 'Headless DOM & Live Mirror', definition_id: 'browser_navigator', status: 'Idle' },
      ]);
      return;
    }
    invoke<any[]>('list_agents', { projectId: activeProjectId })
      .then((activeAgents) => {
        if (activeAgents && Array.isArray(activeAgents)) {
          // Separate primary workspace agents (depth 0 or no parent) from internal sub-agent worker tasks
          const primaryOnly = activeAgents.filter((a) => !a.parent_instance_id || a.depth_level === 0);
          const pool = primaryOnly.length > 0 ? primaryOnly : activeAgents;

          // Deduplicate by name/definition_id so each specialist persona has exactly one clean DM entry
          const seen = new Set<string>();
          const uniqueMembers: TeamMember[] = [];

          for (const a of pool) {
            const personaKey = (a.name || a.definition_id || '').trim().toLowerCase();
            if (!seen.has(personaKey)) {
              seen.add(personaKey);
              uniqueMembers.push({
                id: a.id,
                name: a.name || a.definition_id,
                role: a.role || 'Agent',
                definition_id: a.definition_id,
                status: a.status || 'Idle',
              });
            }
          }
          setTeamMembers(uniqueMembers);
        } else {
          setTeamMembers([]);
        }
      })
      .catch((err) => {
        console.error("Failed to load project agents:", err);
        setTeamMembers([]);
      });
  };

  useEffect(() => {
    loadTeamMembers();
  }, [activeProjectId, agents]);

  const handleCreateChannel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newChannelName.trim() || !activeProjectId) return;
    const cleanTitle = newChannelName.trim().replace(/^#+/, '');
    try {
      await createConversation(activeProjectId, cleanTitle);
      setNewChannelName('');
      setIsAddChannelOpen(false);
      showToast('info', `Channel #${cleanTitle} created`);
    } catch (err: any) {
      console.error("Failed to create channel:", err);
      showToast('error', `Failed to create channel: ${err?.message || err}`);
    }
  };

  const handleCreateCustomAgent = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!customName.trim() || !customRole.trim() || !activeProjectId) return;
    setIsSubmittingAgent(true);
    try {
      const cleanId = customName.trim().toLowerCase().replace(/[^a-z0-9]/g, '_');
      await invoke('create_agent_definition', {
        id: cleanId,
        name: customName.trim(),
        role: customRole.trim(),
        description: customDesc.trim() || customRole.trim(),
        systemInstructions: customPrompt.trim() || `You are ${customName.trim()}, the ${customRole.trim()}. Execute your tasks cleanly.`,
        model: customModel.trim() || 'qwen2.5-coder:3b',
      });

      const spawned = await invoke<any>('create_agent', {
        projectId: activeProjectId,
        definitionId: cleanId,
        prompt: null,
      });

      loadTeamMembers();
      setIsSpawnAgentOpen(false);
      const createdName = customName.trim();
      setCustomName('');
      setCustomRole('');
      setCustomDesc('');
      setCustomPrompt('');

      if (spawned) {
        setActiveDmAgent({
          id: spawned.id,
          name: createdName,
          role: customRole.trim(),
          definition_id: cleanId,
          status: 'Idle',
        });
      }
      showToast('info', `Agent @${createdName} spawned successfully`);
    } catch (err: any) {
      console.error("Failed to define and spawn agent:", err);
      showToast('error', `Failed to spawn agent: ${err?.message || err}`);
    } finally {
      setIsSubmittingAgent(false);
    }
  };

  return (
    <div className="h-full flex flex-col bg-bg-surface select-none border-r border-gray-800 text-xs font-mono">
      {/* Workspace Header */}
      <div className="p-3 border-b border-gray-800 flex items-center justify-between">
        <div className="flex items-center space-x-2 font-bold text-gray-200 truncate">
          <MessageSquare size={14} className="text-brand-primary" />
          <span className="truncate">{activeProject?.name || 'Workspace'}</span>
        </div>
        
        {/* Toggle Mode */}
        <button
          onClick={() => setSidebarMode(sidebarMode === 'team' ? 'files' : 'team')}
          className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
          title={sidebarMode === 'team' ? 'Switch to Workspace Files' : 'Switch to Team Channels'}
        >
          {sidebarMode === 'team' ? <FolderGit2 size={14} /> : <MessageSquare size={14} />}
        </button>
      </div>

      {/* Channels List */}
      <div className="flex-1 overflow-y-auto px-2 py-3 space-y-4">
        <div>
          <div className="flex items-center justify-between px-2 mb-1.5 text-[10px] font-bold tracking-wider text-gray-500 uppercase">
            <span>Channels</span>
            <button
              onClick={() => setIsAddChannelOpen(true)}
              className="p-0.5 rounded hover:bg-gray-800 hover:text-gray-300 text-gray-500"
              title="Add Channel"
            >
              <Plus size={13} />
            </button>
          </div>

          <div className="space-y-0.5">
            {conversations.map((conv) => {
              const isSelected = activeConversationId === conv.id && activeDmAgent === null;
              return (
                <button
                  key={conv.id}
                  onClick={() => {
                    setActiveConversation(conv.id);
                    setActiveTab('chat');
                  }}
                  className={`w-full flex items-center space-x-2 px-2.5 py-1.5 rounded text-left transition-colors ${
                    isSelected
                      ? 'bg-brand-primary/10 text-brand-primary font-semibold'
                      : 'text-gray-400 hover:bg-gray-800/50 hover:text-gray-200'
                  }`}
                >
                  <Hash size={13} className={isSelected ? 'text-brand-primary' : 'text-gray-500'} />
                  <span className="truncate">{conv.title.toLowerCase()}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Direct Messages List */}
        <div>
          <div className="flex items-center justify-between px-2 mb-1.5 text-[10px] font-bold tracking-wider text-gray-500 uppercase">
            <span>Direct Messages</span>
            <button
              onClick={() => setIsSpawnAgentOpen(true)}
              className="p-0.5 rounded hover:bg-gray-800 hover:text-gray-300 text-gray-500"
              title="Spawn or Define Agent"
            >
              <Plus size={13} />
            </button>
          </div>

          <div className="space-y-0.5">
            {teamMembers.length === 0 ? (
              <div className="px-2.5 py-2 text-[11px] text-gray-600 italic">No agents active. Click + to spawn.</div>
            ) : (
              teamMembers.map((member) => {
                const isSelected = activeDmAgent?.id === member.id;
                const isWorking = member.status === 'Running' || member.status === 'Working';
                return (
                  <button
                    key={member.id}
                    onClick={() => {
                      setActiveDmAgent(member);
                      setActiveTab('chat');
                    }}
                    className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded text-left transition-colors ${
                      isSelected
                        ? 'bg-brand-primary/10 text-brand-primary font-semibold'
                        : 'text-gray-400 hover:bg-gray-800/50 hover:text-gray-200'
                    }`}
                  >
                    <div className="flex items-center space-x-2 truncate">
                      <span className="relative flex h-2 w-2">
                        {isWorking && (
                          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-yellow-400 opacity-75" />
                        )}
                        <span className={`relative inline-flex rounded-full h-2 w-2 ${
                          isWorking ? 'bg-yellow-500' : member.status === 'Paused' ? 'bg-gray-500' : 'bg-green-500'
                        }`} />
                      </span>
                      <span className="truncate">@{member.name}</span>
                    </div>
                    <span className="text-[9px] text-gray-500 uppercase px-1 rounded bg-gray-900/60 ml-1 shrink-0">
                      {member.role.split(' ')[0]}
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </div>
      </div>

      {/* Add Channel Modal */}
      {isAddChannelOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-[360px] rounded-lg border border-gray-800 bg-bg-surface p-5 shadow-2xl">
            <h3 className="text-sm font-bold text-gray-200 mb-3">Create Discussion Channel</h3>
            <form onSubmit={handleCreateChannel} className="space-y-3">
              <div>
                <label className="block text-[11px] text-gray-400 mb-1">Channel Name</label>
                <div className="relative flex items-center">
                  <span className="absolute left-2.5 text-gray-500">#</span>
                  <input
                    type="text"
                    value={newChannelName}
                    onChange={(e) => setNewChannelName(e.target.value)}
                    placeholder="e.g. architecture"
                    autoFocus
                    required
                    className="w-full rounded border border-gray-700 bg-bg-base pl-6 pr-3 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
                  />
                </div>
              </div>
              <div className="flex justify-end space-x-2 pt-2">
                <button
                  type="button"
                  onClick={() => setIsAddChannelOpen(false)}
                  className="rounded px-2.5 py-1 text-xs text-gray-400 hover:text-gray-200"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="rounded bg-brand-primary px-3 py-1 text-xs font-semibold text-black hover:opacity-90"
                >
                  Create Channel
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Spawn / Create Agent Modal */}
      {isSpawnAgentOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-[440px] rounded-lg border border-gray-800 bg-bg-surface p-5 shadow-2xl">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-bold text-gray-200">Define & Spawn Sovereign Agent</h3>
              <span className="text-[10px] text-brand-primary font-semibold px-2 py-0.5 rounded bg-brand-primary/10 border border-brand-primary/20">
                Custom Sovereign Agent
              </span>
            </div>

            <form onSubmit={handleCreateCustomAgent} className="space-y-3">
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="block text-[10px] text-gray-400 mb-1">Agent Name</label>
                  <input
                    type="text"
                    value={customName}
                    onChange={(e) => setCustomName(e.target.value)}
                    placeholder="e.g. Coder"
                    required
                    className="w-full rounded border border-gray-700 bg-bg-base px-2.5 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-[10px] text-gray-400 mb-1">Role Title</label>
                  <input
                    type="text"
                    value={customRole}
                    onChange={(e) => setCustomRole(e.target.value)}
                    placeholder="e.g. Systems Engineer"
                    required
                    className="w-full rounded border border-gray-700 bg-bg-base px-2.5 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
                  />
                </div>
              </div>

              <div>
                <label className="block text-[10px] text-gray-400 mb-1">Description</label>
                <input
                  type="text"
                  value={customDesc}
                  onChange={(e) => setCustomDesc(e.target.value)}
                  placeholder="Specialist purpose in this workspace"
                  className="w-full rounded border border-gray-700 bg-bg-base px-2.5 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
                />
              </div>

              <div>
                <label className="block text-[10px] text-gray-400 mb-1">System Instructions</label>
                <textarea
                  value={customPrompt}
                  onChange={(e) => setCustomPrompt(e.target.value)}
                  placeholder="Instructions guiding this agent's reasoning, tools, and tone..."
                  rows={3}
                  className="w-full rounded border border-gray-700 bg-bg-base px-2.5 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none resize-none"
                />
              </div>

              <div>
                <div className="flex items-center justify-between mb-1">
                  <label className="block text-[10px] text-gray-400">LLM Model</label>
                  {isLoadingModels && <span className="text-[9px] text-brand-primary animate-pulse">Detecting models...</span>}
                </div>
                <input
                  list="sidebar-available-models"
                  type="text"
                  value={customModel}
                  onChange={(e) => setCustomModel(e.target.value)}
                  placeholder="e.g. qwen2.5-coder:3b or gpt-4o"
                  className="w-full rounded border border-gray-700 bg-bg-base px-2.5 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
                />
                <datalist id="sidebar-available-models">
                  {availableModels.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>

              <div className="flex justify-end space-x-2 pt-2 border-t border-gray-800">
                <button
                  type="button"
                  onClick={() => setIsSpawnAgentOpen(false)}
                  className="rounded px-3 py-1.5 text-xs text-gray-400 hover:text-gray-200"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmittingAgent || !customName.trim() || !customRole.trim()}
                  className="rounded bg-brand-primary px-3.5 py-1.5 text-xs font-semibold text-black hover:opacity-90 disabled:opacity-50"
                >
                  {isSubmittingAgent ? 'Spawning...' : 'Define & Spawn'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
