import { useState, useEffect } from 'react';
import { 
  Database, 
  Search, 
  Trash2, 
  ArrowUpRight, 
  ArrowDownRight, 
  Sparkles, 
  Brain, 
  BookOpen, 
  Network, 
  List, 
  RefreshCw 
} from 'lucide-react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useUiStore } from '../../store/uiStore';
import LifeGraphView from './LifeGraphView';

export interface MemoryItem {
  id: string;
  project_id: string;
  conversation_id?: string;
  agent_instance_id?: string;
  scope: string;
  lifecycle: string;
  content: string;
  importance: number;
  confidence: number;
  provenance: {
    source_event_id?: string;
    source_message_id?: string;
    source_agent_id?: string;
    creation_reason: string;
  };
  depth_level: number;
  tier: 'Working' | 'Episodic' | 'Semantic' | 'Procedural';
  retrieval_count: number;
  last_retrieved_at?: string;
  expires_at?: string;
  created_at: string;
  updated_at: string;
}

interface TierStat {
  tier: string;
  count: number;
  avg_importance: number;
}

export default function MemoryInspector() {
  const { activeProjectId, showToast } = useUiStore();
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [stats, setStats] = useState<TierStat[]>([]);
  const [selectedTier, setSelectedTier] = useState<string>('All');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedMemory, setSelectedMemory] = useState<MemoryItem | null>(null);
  const [viewMode, setViewMode] = useState<'list' | 'graph'>('list');
  const [loading, setLoading] = useState(false);

  const loadData = async (overrideQuery?: string) => {
    const isRunningInTauri = typeof window !== 'undefined' && (isTauri() || (window as any).__TAURI_INTERNALS__ !== undefined);
    if (!isRunningInTauri) {
      setStats([
        { tier: 'Working', count: 42, avg_importance: 0.84 },
        { tier: 'Episodic', count: 68, avg_importance: 0.76 },
        { tier: 'Semantic', count: 53, avg_importance: 0.91 },
        { tier: 'Procedural', count: 21, avg_importance: 0.98 },
      ]);
      const mockMems: MemoryItem[] = [
        {
          id: 'mem-proc-01',
          project_id: activeProjectId || 'proj-trans4mers-local',
          scope: 'Project',
          lifecycle: 'Active',
          content: 'Rule: Always require human approval token for critical commands mutating filesystem or git branches.',
          importance: 0.99,
          confidence: 1.0,
          provenance: { creation_reason: 'Distilled from operator policy rejection during git checkout' },
          depth_level: 3,
          tier: 'Procedural',
          retrieval_count: 88,
          created_at: new Date(Date.now() - 3600000 * 24 * 3).toISOString(),
          updated_at: new Date().toISOString(),
        },
        {
          id: 'mem-sem-02',
          project_id: activeProjectId || 'proj-trans4mers-local',
          scope: 'Project',
          lifecycle: 'Active',
          content: 'Architecture: Tokio Semaphore enforces a global cap of 8 permits across all agents to prevent OOM.',
          importance: 0.92,
          confidence: 0.96,
          provenance: { creation_reason: 'Extracted from scheduler.rs code audit by Cognitive Memory Specialist' },
          depth_level: 2,
          tier: 'Semantic',
          retrieval_count: 54,
          created_at: new Date(Date.now() - 3600000 * 12).toISOString(),
          updated_at: new Date().toISOString(),
        },
        {
          id: 'mem-epi-03',
          project_id: activeProjectId || 'proj-trans4mers-local',
          scope: 'Conversation',
          lifecycle: 'Active',
          content: 'Execution Event: Cargo test passed with 38 unit and integration tests across storage, engine, and domain.',
          importance: 0.79,
          confidence: 0.95,
          provenance: { creation_reason: 'Nightly dreaming compaction from TerminalOutput broadcast' },
          depth_level: 1,
          tier: 'Episodic',
          retrieval_count: 32,
          created_at: new Date(Date.now() - 3600000 * 4).toISOString(),
          updated_at: new Date().toISOString(),
        },
        {
          id: 'mem-work-04',
          project_id: activeProjectId || 'proj-trans4mers-local',
          scope: 'Agent',
          lifecycle: 'Active',
          content: 'Active Task: Auditing hybrid RRF vector retrieval between LanceDB (384-dim) and SQLite FTS5 lexical index.',
          importance: 0.88,
          confidence: 0.90,
          provenance: { creation_reason: 'ReAct step memory buffer from Cognitive Memory Specialist' },
          depth_level: 0,
          tier: 'Working',
          retrieval_count: 14,
          created_at: new Date(Date.now() - 600000).toISOString(),
          updated_at: new Date().toISOString(),
        }
      ];
      setMemories(selectedTier === 'All' ? mockMems : mockMems.filter(m => m.tier === selectedTier));
      setLoading(false);
      return;
    }

    if (!activeProjectId) return;
    setLoading(true);
    try {
      const q = (overrideQuery !== undefined ? overrideQuery : searchQuery).trim();
      const [mems, st] = await Promise.all([
        q
          ? invoke<MemoryItem[]>('search_memories', {
              projectId: activeProjectId,
              agentId: null,
              query: q,
              limit: 50,
            })
          : invoke<MemoryItem[]>('get_memories', {
              projectId: activeProjectId,
              scope: null,
              tier: selectedTier === 'All' ? null : selectedTier,
              agentId: null,
              limit: 100,
            }),
        invoke<TierStat[]>('get_memory_pyramid_stats', {
          projectId: activeProjectId,
        }),
      ]);

      setMemories(mems || []);
      setStats(st || []);
    } catch (err: any) {
      console.error('Failed to load memory pyramid data:', err);
      showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to load memory pyramid data');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, [activeProjectId, selectedTier]);

  useEffect(() => {
    const timer = setTimeout(() => {
      loadData();
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  const handlePromote = async (item: MemoryItem) => {
    if (!activeProjectId) return;
    const nextTierMap: Record<string, string> = {
      Working: 'Episodic',
      Episodic: 'Semantic',
      Semantic: 'Procedural',
    };
    const nextTier = nextTierMap[item.tier];
    if (!nextTier) return;

    try {
      await invoke('update_memory_tier', {
        projectId: activeProjectId,
        memoryId: item.id,
        newTier: nextTier,
      });
      loadData();
      if (selectedMemory && selectedMemory.id === item.id) {
        setSelectedMemory({ ...selectedMemory, tier: nextTier as any });
      }
      showToast('info', `Promoted memory to ${nextTier}`);
    } catch (err: any) {
      console.error('Failed to promote memory:', err);
      showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to promote memory');
    }
  };

  const handleDemote = async (item: MemoryItem) => {
    if (!activeProjectId) return;
    const prevTierMap: Record<string, string> = {
      Procedural: 'Semantic',
      Semantic: 'Episodic',
      Episodic: 'Working',
    };
    const prevTier = prevTierMap[item.tier];
    if (!prevTier) return;

    try {
      await invoke('update_memory_tier', {
        projectId: activeProjectId,
        memoryId: item.id,
        newTier: prevTier,
      });
      loadData();
      if (selectedMemory && selectedMemory.id === item.id) {
        setSelectedMemory({ ...selectedMemory, tier: prevTier as any });
      }
      showToast('info', `Demoted memory to ${prevTier}`);
    } catch (err: any) {
      console.error('Failed to demote memory:', err);
      showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to demote memory');
    }
  };

  const handleDelete = async (memoryId: string) => {
    if (!activeProjectId) return;
    try {
      await invoke('delete_memory', {
        projectId: activeProjectId,
        memoryId,
      });
      setMemories(prev => prev.filter(m => m.id !== memoryId));
      if (selectedMemory?.id === memoryId) {
        setSelectedMemory(null);
      }
      showToast('info', 'Memory deleted');
    } catch (err: any) {
      console.error('Failed to delete memory:', err);
      showToast('error', typeof err === 'string' ? err : err?.message || 'Failed to delete memory');
    }
  };

  const filteredMemories = memories.filter(m => {
    if (searchQuery.trim() === '') return true;
    const q = searchQuery.toLowerCase();
    return (
      m.content.toLowerCase().includes(q) ||
      m.provenance.creation_reason.toLowerCase().includes(q) ||
      m.scope.toLowerCase().includes(q)
    );
  });

  const getTierBadge = (tier: string) => {
    switch (tier) {
      case 'Procedural':
        return 'bg-emerald-950/40 text-emerald-300 border-emerald-500/40';
      case 'Semantic':
        return 'bg-purple-950/40 text-purple-300 border-purple-500/40';
      case 'Episodic':
        return 'bg-cyan-950/40 text-cyan-300 border-cyan-500/40';
      default:
        return 'bg-amber-950/40 text-amber-300 border-amber-500/40';
    }
  };

  const getTierIcon = (tier: string) => {
    switch (tier) {
      case 'Procedural': return BookOpen;
      case 'Semantic': return Sparkles;
      case 'Episodic': return Brain;
      default: return Database;
    }
  };

  return (
    <div className="h-full flex flex-col bg-zinc-950 text-zinc-200">
      {/* Top Header & Stats */}
      <div className="border-b border-zinc-800 bg-zinc-900/50 p-4 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <Database className="text-brand-primary" size={20} />
            <span className="font-bold text-base text-white">4-Tier Memory Pyramid Inspector</span>
          </div>

          <div className="flex items-center space-x-2">
            <button
              onClick={() => setViewMode(viewMode === 'list' ? 'graph' : 'list')}
              className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-xs font-semibold transition"
            >
              {viewMode === 'list' ? <Network size={14} /> : <List size={14} />}
              <span>{viewMode === 'list' ? 'Life-Graph View' : 'Table View'}</span>
            </button>
            <button
              onClick={() => loadData()}
              disabled={loading}
              className="p-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 transition"
              title="Refresh"
            >
              <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            </button>
          </div>
        </div>

        {/* Pyramid Tier Stats Overview */}
        <div className="grid grid-cols-4 gap-3">
          {[
            { id: 'Procedural', label: 'Procedural', icon: BookOpen, color: 'text-emerald-400', desc: 'Skills & Rules' },
            { id: 'Semantic', label: 'Semantic', icon: Sparkles, color: 'text-purple-400', desc: 'Distilled Knowledge' },
            { id: 'Episodic', label: 'Episodic', icon: Brain, color: 'text-cyan-400', desc: 'Execution History' },
            { id: 'Working', label: 'Working', icon: Database, color: 'text-amber-400', desc: 'Active Workspace' },
          ].map(t => {
            const stat = stats.find(s => s.tier === t.id);
            const count = stat ? stat.count : 0;
            const avgImp = stat ? (stat.avg_importance * 100).toFixed(0) : '0';
            const Icon = t.icon;
            const isSelected = selectedTier === t.id;

            return (
              <div
                key={t.id}
                onClick={() => setSelectedTier(selectedTier === t.id ? 'All' : t.id)}
                className={`cursor-pointer p-3 rounded-lg border transition select-none ${
                  isSelected
                    ? 'border-white bg-zinc-800/80'
                    : 'border-zinc-800 bg-zinc-900/30 hover:border-zinc-700'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-1.5">
                    <Icon size={14} className={t.color} />
                    <span className="font-bold text-xs text-white">{t.label}</span>
                  </div>
                  <span className="font-mono text-sm font-bold text-white">{count}</span>
                </div>
                <div className="flex items-center justify-between text-[11px] text-zinc-500 mt-1">
                  <span>{t.desc}</span>
                  <span>Avg: {avgImp}%</span>
                </div>
              </div>
            );
          })}
        </div>

        {/* Search & Filter Bar */}
        <div className="flex items-center space-x-3">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-3 top-2.5 text-zinc-500" />
            <input
              type="text"
              placeholder="Search memories by content, reason, or scope..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="w-full pl-9 pr-3 py-1.5 rounded-lg bg-zinc-900 border border-zinc-800 text-xs text-white focus:outline-none focus:border-zinc-600"
            />
          </div>
          {selectedTier !== 'All' && (
            <button
              onClick={() => setSelectedTier('All')}
              className="text-xs text-zinc-400 hover:text-white underline decoration-zinc-600"
            >
              Reset Filter
            </button>
          )}
        </div>
      </div>

      {/* Main View Area */}
      <div className="flex-1 overflow-hidden flex">
        {viewMode === 'graph' ? (
          <LifeGraphView
            memories={filteredMemories}
            onSelectNode={(id) => {
              const found = memories.find(m => m.id === id);
              if (found) setSelectedMemory(found);
            }}
            selectedId={selectedMemory?.id}
          />
        ) : (
          <div className="flex-1 overflow-y-auto p-4 space-y-2">
            {filteredMemories.length === 0 ? (
              <div className="text-center py-12 text-zinc-600 text-xs">
                No memories found matching the current filters.
              </div>
            ) : (
              filteredMemories.map(item => {
                const Icon = getTierIcon(item.tier);
                const isSelected = selectedMemory?.id === item.id;
                return (
                  <div
                    key={item.id}
                    onClick={() => setSelectedMemory(item)}
                    className={`p-3 rounded-lg border cursor-pointer transition select-none flex items-start justify-between space-x-4 ${
                      isSelected
                        ? 'border-brand-primary bg-zinc-900 shadow-md'
                        : 'border-zinc-800/80 bg-zinc-900/40 hover:bg-zinc-900/80'
                    }`}
                  >
                    <div className="flex-1 min-w-0 space-y-1">
                      <div className="flex items-center space-x-2">
                        <span className={`text-[11px] font-bold px-2 py-0.5 rounded border flex items-center space-x-1 ${getTierBadge(item.tier)}`}>
                          <Icon size={12} />
                          <span>{item.tier}</span>
                        </span>
                        <span className="text-[11px] font-mono text-zinc-400 bg-zinc-800/60 px-1.5 py-0.5 rounded">
                          {item.scope}
                        </span>
                        <span className="text-[11px] text-zinc-500">
                          Retrievals: <strong className="text-zinc-300 font-mono">{item.retrieval_count}</strong>
                        </span>
                        <span className="text-[11px] text-zinc-500">
                          Importance: <strong className="text-zinc-300">{(item.importance * 100).toFixed(0)}%</strong>
                        </span>
                      </div>

                      <div className="text-xs font-mono text-zinc-200 line-clamp-2 leading-relaxed">
                        {item.content}
                      </div>

                      <div className="text-[11px] text-zinc-500 truncate">
                        Reason: {item.provenance.creation_reason} • {new Date(item.created_at).toLocaleDateString()}
                      </div>
                    </div>

                    <div className="flex items-center space-x-1 shrink-0 pt-1" onClick={e => e.stopPropagation()}>
                      {item.tier !== 'Procedural' && (
                        <button
                          onClick={() => handlePromote(item)}
                          className="p-1.5 rounded hover:bg-zinc-800 text-zinc-400 hover:text-emerald-300 transition"
                          title="Promote to higher tier"
                        >
                          <ArrowUpRight size={14} />
                        </button>
                      )}
                      {item.tier !== 'Working' && (
                        <button
                          onClick={() => handleDemote(item)}
                          className="p-1.5 rounded hover:bg-zinc-800 text-zinc-400 hover:text-amber-300 transition"
                          title="Demote to lower tier"
                        >
                          <ArrowDownRight size={14} />
                        </button>
                      )}
                      <button
                        onClick={() => handleDelete(item.id)}
                        className="p-1.5 rounded hover:bg-zinc-800 text-zinc-400 hover:text-red-400 transition"
                        title="Delete memory"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        )}

        {/* Right Detail Inspector Panel */}
        {selectedMemory && (
          <div className="w-80 border-l border-zinc-800 bg-zinc-900/60 p-4 overflow-y-auto space-y-4 text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
              <span className="font-bold text-white text-sm">Memory Details</span>
              <button
                onClick={() => setSelectedMemory(null)}
                className="text-zinc-400 hover:text-white"
              >
                ✕
              </button>
            </div>

            <div className="space-y-1">
              <div className="text-zinc-500 font-medium">Content</div>
              <div className="p-2.5 rounded bg-black/40 border border-zinc-800 font-mono text-zinc-200 leading-relaxed whitespace-pre-wrap">
                {selectedMemory.content}
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2 text-zinc-400">
              <div className="p-2 rounded bg-zinc-900/80 border border-zinc-800/80">
                <div className="text-[10px] text-zinc-500">Tier</div>
                <div className="font-bold text-white mt-0.5">{selectedMemory.tier}</div>
              </div>
              <div className="p-2 rounded bg-zinc-900/80 border border-zinc-800/80">
                <div className="text-[10px] text-zinc-500">Scope</div>
                <div className="font-bold text-white mt-0.5">{selectedMemory.scope}</div>
              </div>
              <div className="p-2 rounded bg-zinc-900/80 border border-zinc-800/80">
                <div className="text-[10px] text-zinc-500">Importance</div>
                <div className="font-bold text-white mt-0.5">{(selectedMemory.importance * 100).toFixed(0)}%</div>
              </div>
              <div className="p-2 rounded bg-zinc-900/80 border border-zinc-800/80">
                <div className="text-[10px] text-zinc-500">Retrieval Count</div>
                <div className="font-bold text-white mt-0.5 font-mono">{selectedMemory.retrieval_count}</div>
              </div>
            </div>

            <div className="space-y-1">
              <div className="text-zinc-500 font-medium">Provenance & Context</div>
              <div className="p-2 rounded bg-zinc-900/40 border border-zinc-800 space-y-1 text-zinc-400">
                <div>Reason: <span className="text-zinc-200">{selectedMemory.provenance.creation_reason}</span></div>
                {selectedMemory.provenance.source_agent_id && (
                  <div>Agent: <span className="font-mono text-zinc-200">{selectedMemory.provenance.source_agent_id}</span></div>
                )}
                {selectedMemory.provenance.source_event_id && (
                  <div>Event: <span className="font-mono text-zinc-200">{selectedMemory.provenance.source_event_id}</span></div>
                )}
                <div>Created: <span className="text-zinc-200">{new Date(selectedMemory.created_at).toLocaleString()}</span></div>
                {selectedMemory.last_retrieved_at && (
                  <div>Last Retrieved: <span className="text-zinc-200">{new Date(selectedMemory.last_retrieved_at).toLocaleString()}</span></div>
                )}
              </div>
            </div>

            <div className="pt-2 flex space-x-2">
              {selectedMemory.tier !== 'Procedural' && (
                <button
                  onClick={() => handlePromote(selectedMemory)}
                  className="flex-1 flex items-center justify-center space-x-1 py-2 rounded bg-emerald-600/30 hover:bg-emerald-600/50 text-emerald-300 border border-emerald-500/30 font-semibold"
                >
                  <ArrowUpRight size={14} />
                  <span>Promote</span>
                </button>
              )}
              {selectedMemory.tier !== 'Working' && (
                <button
                  onClick={() => handleDemote(selectedMemory)}
                  className="flex-1 flex items-center justify-center space-x-1 py-2 rounded bg-amber-600/30 hover:bg-amber-600/50 text-amber-300 border border-amber-500/30 font-semibold"
                >
                  <ArrowDownRight size={14} />
                  <span>Demote</span>
                </button>
              )}
              <button
                onClick={() => handleDelete(selectedMemory.id)}
                className="p-2 rounded bg-red-600/30 hover:bg-red-600/50 text-red-300 border border-red-500/30"
                title="Delete"
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
