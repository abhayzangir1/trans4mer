import { useState } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { useUiStore } from "../../store/uiStore";
import { useConversationStore } from "../../store/conversationStore";
import { Terminal, FolderGit2, MessagesSquare, Code2, Users, Settings, Hash, Bot, Database, Cpu, Compass, Globe, BookOpen, Layers, PanelLeft } from "lucide-react";
import XTermWrapper from "../terminal/XTermWrapper";
import MessageList from "../chat/MessageList";
import ChatBox from "../chat/ChatBox";
import CodeEditor from "../editor/CodeEditor";
import ApprovalWidget from "../approval/ApprovalWidget";
import FileExplorer from "./FileExplorer";
import TeamSidebar from "./TeamSidebar";
import SettingsModal from "../settings/SettingsModal";
import AgentDetailPanel from "../agent/AgentDetailPanel";
import MemoryInspector from "../panels/MemoryInspector";
import FleetDashboard from "../panels/FleetDashboard";
import DeepResearchPane from "../panels/DeepResearchPane";
import VisualSwarmDesigner from "../panels/VisualSwarmDesigner";
import LiveMirrorPane from "../panels/LiveMirrorPane";
import DocumentRagPanel from "../panels/DocumentRagPanel";
import InteractiveArtifactsPanel from "../panels/InteractiveArtifactsPanel";
import { WorkspaceBoundary } from "./WorkspaceBoundary";
import { PanelBoundary } from "./PanelBoundary";
import { WidgetBoundary } from "./WidgetBoundary";

export default function WorkspaceShell() {
  const { isSidebarOpen, toggleSidebar, selectedFilePath, activeProjectId, activeTab, setActiveTab } = useUiStore();
  const { sidebarMode, setSidebarMode, activeConversationId, activeDmAgent, conversations } = useConversationStore();
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  const activeChannel = conversations.find(c => c.id === activeConversationId);
  const currentTitle = activeDmAgent 
    ? `@${activeDmAgent.name}` 
    : `#${activeChannel?.title.toLowerCase() || 'general'}`;
  const currentSubtitle = activeDmAgent 
    ? `Direct message with ${activeDmAgent.name} (${activeDmAgent.role})` 
    : `Shared blackboard channel for ${activeChannel?.title || 'General'}`;

  return (
    <WorkspaceBoundary>
    <div className="h-full w-full relative bg-bg-base text-text-primary">
      <ApprovalWidget />
      <SettingsModal isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
      
      <PanelGroup direction="horizontal" className="h-full w-full">
        {/* LEFT: Team Channels & DMs OR Workspace File Explorer */}
        {isSidebarOpen && (
          <>
            <Panel defaultSize={20} minSize={15} maxSize={30} className="bg-bg-surface border-r border-gray-800 flex flex-col">
              <PanelBoundary panelName={sidebarMode === 'team' ? "Team Channels" : "Workspace Files"}>
                {sidebarMode === 'team' ? (
                  <TeamSidebar />
                ) : (
                  <div className="h-full flex flex-col">
                    <div className="p-3 border-b border-gray-800 flex items-center justify-between text-xs font-semibold uppercase tracking-wider text-gray-500 font-mono">
                      <div className="flex items-center space-x-2">
                        <FolderGit2 size={14} />
                        <span>Files</span>
                      </div>
                      <button
                        onClick={() => setSidebarMode('team')}
                        className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200"
                        title="Switch to Team Channels"
                      >
                        <MessagesSquare size={14} />
                      </button>
                    </div>
                    <div className="flex-1 overflow-hidden flex flex-col">
                      <FileExplorer />
                    </div>
                  </div>
                )}
              </PanelBoundary>
            </Panel>
            <PanelResizeHandle className="w-1 bg-gray-800 hover:bg-brand-primary/50 transition-colors cursor-col-resize" />
          </>
        )}

        {/* CENTER: Main Team Chat / Code Editor / Swarm Topology */}
        <Panel className="flex flex-col bg-bg-base relative">
          <PanelBoundary panelName="Center Workspace">
            <div className="h-full flex flex-col">
              {/* Channel Header Bar */}
              <div className="h-12 border-b border-gray-800 bg-bg-surface px-4 flex items-center justify-between font-mono select-none">
                <div className="flex items-center space-x-2.5 truncate">
                  <button
                    onClick={toggleSidebar}
                    className="p-1 rounded hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors shrink-0"
                    title={isSidebarOpen ? "Collapse sidebar" : "Expand sidebar"}
                  >
                    <PanelLeft size={16} />
                  </button>
                  {activeDmAgent ? (
                    <Bot size={18} className="text-brand-primary shrink-0" />
                  ) : (
                    <Hash size={18} className="text-gray-400 shrink-0" />
                  )}
                  <div className="truncate">
                    <div className="font-bold text-sm text-gray-100 truncate">{currentTitle}</div>
                    <div className="text-[10px] text-gray-500 truncate">{currentSubtitle}</div>
                  </div>
                </div>

                <div className="flex items-center space-x-1">
                  <button 
                    onClick={() => setActiveTab('chat')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'chat' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <MessagesSquare size={13} />
                    <span>Chat</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('code')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'code' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Code2 size={13} />
                    <span>Code</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('swarm')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'swarm' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Users size={13} />
                    <span>Swarm</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('fleet')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'fleet' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Cpu size={13} />
                    <span>Fleet</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('research')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'research' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Compass size={13} />
                    <span>Research</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('browser')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'browser' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Globe size={13} />
                    <span>Browser</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('memory')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'memory' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Database size={13} />
                    <span>Memory</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('docs')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'docs' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <BookOpen size={13} />
                    <span>Docs</span>
                  </button>
                  <button 
                    onClick={() => setActiveTab('artifacts')}
                    className={`px-3 py-1.5 text-xs font-medium rounded flex items-center space-x-1.5 transition-colors ${
                      activeTab === 'artifacts' ? 'bg-brand-primary/15 text-brand-primary font-semibold' : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                    }`}
                  >
                    <Layers size={13} />
                    <span>Artifacts</span>
                  </button>
                  <button 
                    onClick={() => setIsSettingsOpen(true)}
                    className="p-1.5 text-gray-400 hover:text-gray-200 rounded hover:bg-gray-800 ml-1 transition-colors"
                    title="Settings"
                  >
                    <Settings size={14} />
                  </button>
                </div>
              </div>

              {/* Tab Content */}
              <div className="flex-1 overflow-hidden flex flex-col relative">
                {activeTab === 'chat' && (
                  <div className="h-full flex flex-col">
                    <MessageList />
                    <ChatBox />
                  </div>
                )}
                {activeTab === 'code' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Monaco Editor">
                      <CodeEditor filePath={selectedFilePath} />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'swarm' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Visual Swarm Designer">
                      <VisualSwarmDesigner projectId={activeProjectId} />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'fleet' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Fleet Dashboard">
                      <FleetDashboard />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'research' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Deep Research Engine">
                      <DeepResearchPane />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'browser' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Per-Space Live Mirror">
                      <LiveMirrorPane />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'memory' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Memory Pyramid Inspector">
                      <MemoryInspector />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'docs' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Document RAG Substrate">
                      <DocumentRagPanel />
                    </WidgetBoundary>
                  </div>
                )}
                {activeTab === 'artifacts' && (
                  <div className="h-full relative">
                    <WidgetBoundary name="Interactive Artifacts">
                      <InteractiveArtifactsPanel />
                    </WidgetBoundary>
                  </div>
                )}
              </div>
            </div>
          </PanelBoundary>
        </Panel>

        <PanelResizeHandle className="w-1 bg-gray-800 hover:bg-brand-primary/50 transition-colors cursor-col-resize" />

        {/* RIGHT: Agent Inspector & Integrated Terminal */}
        <Panel defaultSize={28} minSize={20} maxSize={45} className="bg-bg-surface border-l border-gray-800 flex flex-col">
          <PanelBoundary panelName="Swarm Details & Terminal">
            <div className="flex-1 flex flex-col border-b border-gray-800 overflow-hidden">
              <AgentDetailPanel />
            </div>
            <div className="h-64 flex flex-col shrink-0">
              <div className="p-2 border-b border-gray-800 flex items-center space-x-2 text-xs font-semibold uppercase tracking-wider text-gray-500 bg-black/20 font-mono">
                <Terminal size={14} />
                <span>Integrated Terminal</span>
              </div>
              <div className="flex-1 bg-[#1e1e1e] p-2 relative">
                <div className="absolute inset-2">
                  <WidgetBoundary name="XTerm.js">
                    <XTermWrapper />
                  </WidgetBoundary>
                </div>
              </div>
            </div>
          </PanelBoundary>
        </Panel>
      </PanelGroup>
    </div>
    </WorkspaceBoundary>
  );
}
