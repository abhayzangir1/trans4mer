import { useCallback, useEffect, useState } from 'react';
import ReactFlow, {
  MiniMap,
  Controls,
  Background,
  useNodesState,
  useEdgesState,
  Node,
  Edge,
  Position,
} from 'reactflow';
import dagre from 'dagre';
import 'reactflow/dist/style.css';
import { Send, Bot, MessageSquare } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useAgents } from '../../hooks/useAgents';
import { useSwarmStore } from '../../store/swarmStore';
import { useConversationStore } from '../../store/conversationStore';
import { useUiStore } from '../../store/uiStore';
import AgentSteerDrawer from '../workspace/AgentSteerDrawer';

const dagreGraph = new dagre.graphlib.Graph();
dagreGraph.setDefaultEdgeLabel(() => ({}));

const nodeWidth = 220;
const nodeHeight = 84;

const getLayoutedElements = (nodes: Node[], edges: Edge[], direction = 'TB') => {
  const isHorizontal = direction === 'LR';
  dagreGraph.setGraph({ rankdir: direction });

  nodes.forEach((node) => {
    dagreGraph.setNode(node.id, { width: nodeWidth, height: nodeHeight });
  });

  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  nodes.forEach((node) => {
    const nodeWithPosition = dagreGraph.node(node.id);
    node.targetPosition = isHorizontal ? Position.Left : Position.Top;
    node.sourcePosition = isHorizontal ? Position.Right : Position.Bottom;

    // Shift to align center point to top left for reactflow
    node.position = {
      x: nodeWithPosition.x - nodeWidth / 2,
      y: nodeWithPosition.y - nodeHeight / 2,
    };

    return node;
  });

  return { nodes, edges };
};

export default function SwarmMap({ projectId }: { projectId: string | null }) {
  const { agents } = useAgents(projectId);
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);
  const { selectedAgentId, setSelectedAgent } = useSwarmStore();
  const { activeConversationId, setActiveDmAgent } = useConversationStore();
  const { setActiveTab } = useUiStore();

  const [inlinePrompt, setInlinePrompt] = useState('');
  const [isSending, setIsSending] = useState(false);

  const selectedAgent = agents.find(a => a.id === selectedAgentId);

  // Deterministically layout nodes based on hierarchy
  useEffect(() => {
    const initialNodes: Node[] = agents.map((agent) => {
      const isSelected = agent.id === selectedAgentId;
      const displayName = agent.name || (agent.definitionId ? agent.definitionId.charAt(0).toUpperCase() + agent.definitionId.slice(1) : 'Agent');
      const displayRole = agent.role || 'Specialist';
      const isWorking = agent.status === 'Running' || agent.status === 'Working';

      return {
        id: agent.id,
        position: { x: 0, y: 0 },
        data: {
          label: (
            <div className="flex flex-col items-center justify-center p-1 text-xs font-mono">
              <div className="flex items-center space-x-1.5 font-bold truncate">
                <span className={`w-2 h-2 rounded-full ${isWorking ? 'bg-yellow-400 animate-ping' : agent.status === 'Paused' ? 'bg-gray-500' : 'bg-green-400'}`} />
                <span className="truncate">{displayName}</span>
              </div>
              <div className="text-[10px] text-gray-400 truncate mt-0.5">{displayRole}</div>
              <div className="text-[9px] text-gray-500 uppercase mt-0.5 tracking-wider font-semibold">[{agent.status}]</div>
            </div>
          )
        },
        style: {
          background: isSelected ? 'rgba(59, 130, 246, 0.25)' : '#18181b',
          color: '#f4f4f5',
          border: isSelected ? '2px solid #3b82f6' : '1px solid #27272a',
          borderRadius: '8px',
          width: 220,
          padding: 8,
          boxShadow: isSelected ? '0 0 15px rgba(59, 130, 246, 0.4)' : 'none',
        }
      };
    });

    // Construct real edges based on parentInstanceId
    const initialEdges: Edge[] = agents
      .filter(a => a.parentId)
      .map(a => ({
        id: `edge-${a.parentId}-${a.id}`,
        source: a.parentId!,
        target: a.id,
        animated: true,
        style: { stroke: '#3b82f6', strokeWidth: 2 }
      }));

    const { nodes: layoutedNodes, edges: layoutedEdges } = getLayoutedElements(
      initialNodes,
      initialEdges
    );

    setNodes([...layoutedNodes]);
    setEdges([...layoutedEdges]);
  }, [agents, setNodes, setEdges, selectedAgentId]);

  const onNodeClick = useCallback((_: any, node: Node) => {
    setSelectedAgent(node.id);
  }, [setSelectedAgent]);

  const onPaneClick = useCallback(() => {
    setSelectedAgent(null);
  }, [setSelectedAgent]);

  const handleSendInlineTask = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inlinePrompt.trim() || !selectedAgent || !projectId) return;

    setIsSending(true);
    try {
      await invoke('send_message', {
        projectId,
        conversationId: activeConversationId || projectId,
        channelId: selectedAgent.id,
        content: inlinePrompt.trim(),
        mentions: [],
      });
      setInlinePrompt('');
    } catch (err) {
      console.error("Failed to send inline task to agent:", err);
    } finally {
      setIsSending(false);
    }
  };

  const handleOpenDm = () => {
    if (!selectedAgent) return;
    setActiveDmAgent({
      id: selectedAgent.id,
      name: selectedAgent.name || selectedAgent.definitionId,
      role: selectedAgent.role || 'Specialist',
      definition_id: selectedAgent.definitionId,
      status: selectedAgent.status || 'Idle',
    });
    setActiveTab('chat');
  };

  return (
    <div className="h-full w-full bg-bg-base relative flex flex-col">
      <div className="flex-1 relative">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          fitView
        >
          <Controls />
          <MiniMap nodeStrokeColor="#3b82f6" nodeColor="#27272a" maskColor="rgba(9, 9, 11, 0.85)" />
          <Background color="#27272a" gap={16} />
        </ReactFlow>
      </div>

      {/* Floating Dispatch Bar for Selected Agent */}
      {selectedAgent && (
        <div className="absolute bottom-4 left-4 right-4 z-20 mx-auto max-w-2xl bg-bg-surface/95 backdrop-blur-md border border-gray-800 rounded-lg p-3 shadow-2xl font-mono text-xs">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center space-x-2 text-gray-200">
              <Bot size={15} className="text-brand-primary" />
              <span className="font-bold">@{selectedAgent.name || selectedAgent.definitionId}</span>
              <span className="text-[10px] text-gray-500 uppercase px-1 rounded bg-gray-900">
                {selectedAgent.role || 'Agent'}
              </span>
            </div>
            <button
              onClick={handleOpenDm}
              className="flex items-center space-x-1 text-[11px] text-brand-primary hover:underline"
            >
              <MessageSquare size={12} />
              <span>Open 1:1 Direct Message</span>
            </button>
          </div>

          <form onSubmit={handleSendInlineTask} className="flex items-center space-x-2">
            <input
              type="text"
              value={inlinePrompt}
              onChange={(e) => setInlinePrompt(e.target.value)}
              placeholder={`Dispatch direct task to @${selectedAgent.name || selectedAgent.definitionId}...`}
              className="flex-1 rounded border border-gray-700 bg-bg-base px-3 py-1.5 text-xs text-gray-200 focus:border-brand-primary focus:outline-none"
            />
            <button
              type="submit"
              disabled={isSending || !inlinePrompt.trim()}
              className="flex items-center space-x-1.5 rounded bg-brand-primary px-3 py-1.5 text-xs font-semibold text-black hover:opacity-90 disabled:opacity-50"
            >
              <Send size={13} />
              <span>{isSending ? 'Sending...' : 'Dispatch'}</span>
            </button>
          </form>
        </div>
      )}

      {/* Slide-out Agent Steering & Live Tailing Drawer */}
      {selectedAgent && projectId && (
        <AgentSteerDrawer
          projectId={projectId}
          agent={selectedAgent}
          onClose={() => setSelectedAgent(null)}
        />
      )}
    </div>
  );
}
