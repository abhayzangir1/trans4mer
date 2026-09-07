import { create } from 'zustand';
import { useProjectStore } from './projectStore';
import { useConversationStore } from './conversationStore';

export interface ToastNotification {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  message: string;
}

interface UiState {
  isSidebarOpen: boolean;
  activeProjectId: string | null;
  selectedFilePath: string | null;
  activeTab: 'chat' | 'code' | 'swarm' | 'memory' | 'fleet' | 'research' | 'browser' | 'docs' | 'artifacts';
  toasts: ToastNotification[];
  toggleSidebar: () => void;
  setActiveProject: (id: string) => void;
  setSelectedFile: (path: string | null) => void;
  setActiveTab: (tab: 'chat' | 'code' | 'swarm' | 'memory' | 'fleet' | 'research' | 'browser' | 'docs' | 'artifacts') => void;
  showToast: (type: 'info' | 'success' | 'warning' | 'error', message: string) => void;
  removeToast: (id: string) => void;
}

export const useUiStore = create<UiState>((set) => ({
  isSidebarOpen: true,
  activeProjectId: useProjectStore.getState().activeProjectId,
  selectedFilePath: null,
  activeTab: 'chat',
  toasts: [],
  toggleSidebar: () => set((state) => ({ isSidebarOpen: !state.isSidebarOpen })),
  setActiveProject: (id) => {
    set({ activeProjectId: id, selectedFilePath: null });
    useProjectStore.getState().setActiveProject(id);
    useConversationStore.getState().clearConversations();
    useConversationStore.getState().fetchConversations(id);
  },
  setSelectedFile: (path) => set((state) => ({
    selectedFilePath: path,
    activeTab: path ? 'code' : state.activeTab,
  })),
  setActiveTab: (tab) => set({ activeTab: tab }),
  showToast: (type, message) => {
    const id = Math.random().toString(36).substring(2, 9);
    set((state) => ({ toasts: [...state.toasts, { id, type, message }] }));
    setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
    }, 4500);
  },
  removeToast: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
}));

// One-way synchronization from projectStore to uiStore
useProjectStore.subscribe((projectState) => {
  if (useUiStore.getState().activeProjectId !== projectState.activeProjectId) {
    useUiStore.setState({ activeProjectId: projectState.activeProjectId, selectedFilePath: null });
    useConversationStore.getState().clearConversations();
    if (projectState.activeProjectId) {
      useConversationStore.getState().fetchConversations(projectState.activeProjectId);
    }
  }
});
