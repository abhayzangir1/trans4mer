import { useMemo } from 'react';
import { Database, Brain, Sparkles, BookOpen } from 'lucide-react';

interface MemoryNode {
  id: string;
  tier: 'Working' | 'Episodic' | 'Semantic' | 'Procedural';
  content: string;
  importance: number;
  retrieval_count: number;
  agent_instance_id?: string;
}

interface LifeGraphViewProps {
  memories: MemoryNode[];
  onSelectNode: (id: string) => void;
  selectedId?: string | null;
}

export default function LifeGraphView({ memories, onSelectNode, selectedId }: LifeGraphViewProps) {
  const grouped = useMemo(() => {
    const tiers: Record<string, MemoryNode[]> = {
      Procedural: [],
      Semantic: [],
      Episodic: [],
      Working: [],
    };
    for (const m of memories) {
      if (tiers[m.tier]) {
        tiers[m.tier].push(m);
      } else {
        tiers.Working.push(m);
      }
    }
    return tiers;
  }, [memories]);

  const tierMeta = {
    Procedural: { label: 'Procedural Memory (Skills & Rules)', icon: BookOpen, color: 'text-emerald-400', border: 'border-emerald-500/40', bg: 'bg-emerald-950/20' },
    Semantic: { label: 'Semantic Memory (Distilled Knowledge)', icon: Sparkles, color: 'text-purple-400', border: 'border-purple-500/40', bg: 'bg-purple-950/20' },
    Episodic: { label: 'Episodic Memory (Execution Experiences)', icon: Brain, color: 'text-cyan-400', border: 'border-cyan-500/40', bg: 'bg-cyan-950/20' },
    Working: { label: 'Working Memory (Active Context)', icon: Database, color: 'text-amber-400', border: 'border-amber-500/40', bg: 'bg-amber-950/20' },
  };

  return (
    <div className="h-full flex flex-col p-6 overflow-y-auto space-y-6 bg-zinc-950">
      <div className="text-xs text-zinc-400">
        Visualizing the 4-Tier Memory Pyramid hierarchy. Click any memory node to inspect details and provenance.
      </div>

      {(['Procedural', 'Semantic', 'Episodic', 'Working'] as const).map(tierKey => {
        const meta = tierMeta[tierKey];
        const Icon = meta.icon;
        const nodes = grouped[tierKey] || [];

        return (
          <div key={tierKey} className={`rounded-xl border p-4 transition ${meta.border} ${meta.bg}`}>
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center space-x-2">
                <Icon size={16} className={meta.color} />
                <span className={`font-bold text-xs uppercase tracking-wider ${meta.color}`}>
                  {meta.label}
                </span>
              </div>
              <span className="text-xs text-zinc-400 font-mono">
                {nodes.length} {nodes.length === 1 ? 'node' : 'nodes'}
              </span>
            </div>

            {nodes.length === 0 ? (
              <div className="text-xs text-zinc-600 italic py-2">No memories currently in this tier.</div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                {nodes.map(node => {
                  const isSelected = selectedId === node.id;
                  return (
                    <div
                      key={node.id}
                      onClick={() => onSelectNode(node.id)}
                      className={`p-3 rounded-lg border cursor-pointer transition select-none ${
                        isSelected
                          ? 'border-white bg-zinc-800/80 shadow-lg text-white'
                          : 'border-zinc-800/80 bg-zinc-900/60 text-zinc-300 hover:border-zinc-700 hover:bg-zinc-800/50'
                      }`}
                    >
                      <div className="text-xs font-mono line-clamp-3 leading-relaxed mb-2">
                        {node.content}
                      </div>
                      <div className="flex items-center justify-between text-[11px] text-zinc-400 border-t border-zinc-800/60 pt-2 mt-2">
                        <span>Imp: {(node.importance * 100).toFixed(0)}%</span>
                        <span className="font-mono">R: {node.retrieval_count}</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
