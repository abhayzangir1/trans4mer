import { useEffect, useState } from 'react';
import { Bot } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

export interface MentionOption {
  name: string;
  role: string;
  description: string;
}

interface MentionAutocompleteProps {
  query: string;
  onSelect: (name: string) => void;
  onClose: () => void;
}

export default function MentionAutocomplete({ query, onSelect, onClose }: MentionAutocompleteProps) {
  const [agents, setAgents] = useState<MentionOption[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);

  useEffect(() => {
    invoke<any[]>('list_agent_definitions')
      .then((defs) => {
        if (defs && Array.isArray(defs)) {
          setAgents(defs.map(d => ({
            name: d.name,
            role: d.role,
            description: d.description || '',
          })));
        }
      })
      .catch((err) => console.error("Failed to load agent definitions for autocomplete:", err));
  }, []);

  const filtered = agents.filter(a =>
    a.name.toLowerCase().includes(query.toLowerCase()) ||
    a.role.toLowerCase().includes(query.toLowerCase())
  );

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (filtered.length === 0) return;

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % filtered.length);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + filtered.length) % filtered.length);
      } else if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        if (filtered[selectedIndex]) {
          onSelect(filtered[selectedIndex].name);
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [filtered, selectedIndex, onSelect, onClose]);

  if (filtered.length === 0) return null;

  return (
    <div className="absolute bottom-full left-0 mb-2 w-80 rounded-lg border border-gray-700 bg-bg-surface p-1 shadow-2xl z-50 text-xs font-mono">
      <div className="px-2 py-1 text-[10px] font-semibold text-gray-500 uppercase tracking-wider border-b border-gray-800">
        Mention an Agent
      </div>
      <div className="max-h-48 overflow-y-auto py-1 space-y-0.5">
        {filtered.map((agent, idx) => {
          const isSelected = idx === selectedIndex;
          return (
            <button
              key={agent.name}
              type="button"
              onClick={() => onSelect(agent.name)}
              onMouseEnter={() => setSelectedIndex(idx)}
              className={`w-full flex items-start space-x-2.5 px-2.5 py-1.5 rounded text-left transition-colors ${
                isSelected ? 'bg-brand-primary/15 text-brand-primary' : 'text-gray-300 hover:bg-gray-800/60'
              }`}
            >
              <Bot size={15} className={`mt-0.5 shrink-0 ${isSelected ? 'text-brand-primary' : 'text-gray-400'}`} />
              <div className="flex-1 truncate">
                <div className="flex items-center space-x-1.5">
                  <span className="font-bold">@{agent.name}</span>
                  <span className="text-[10px] text-gray-500">[{agent.role}]</span>
                </div>
                <div className="text-[10px] text-gray-400 truncate">{agent.description}</div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
