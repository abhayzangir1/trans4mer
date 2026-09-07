import React, { useState, useRef } from 'react';
import { SendHorizontal, AtSign } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { useUiStore } from '../../store/uiStore';
import { useConversationStore } from '../../store/conversationStore';
import MentionAutocomplete from './MentionAutocomplete';

export default function ChatBox() {
  const [input, setInput] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [showMentionMenu, setShowMentionMenu] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const activeProjectId = useUiStore(state => state.activeProjectId);
  const showToast = useUiStore(state => state.showToast);
  const { activeConversationId, activeDmAgent, conversations } = useConversationStore();

  const activeChannel = conversations.find(c => c.id === activeConversationId);
  const channelTitle = activeDmAgent 
    ? `@${activeDmAgent.name}` 
    : `#${activeChannel?.title.toLowerCase() || 'general'}`;

  // Handle typing to detect '@' for mention autocomplete
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value;
    setInput(val);

    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 120)}px`;
    }

    const cursorPos = e.target.selectionStart || val.length;
    const textBeforeCursor = val.slice(0, cursorPos);
    const match = textBeforeCursor.match(/@([a-zA-Z0-9_-]*)$/);

    if (match) {
      setShowMentionMenu(true);
      setMentionQuery(match[1]);
    } else {
      setShowMentionMenu(false);
    }
  };

  const handleSelectMention = (agentName: string) => {
    if (!textareaRef.current) return;
    const cursorPos = textareaRef.current.selectionStart || input.length;
    const textBefore = input.slice(0, cursorPos);
    const textAfter = input.slice(cursorPos);
    
    // Replace the trailing @query with @AgentName 
    const replacedBefore = textBefore.replace(/@([a-zA-Z0-9_-]*)$/, `@${agentName} `);
    const newInput = replacedBefore + textAfter;
    setInput(newInput);
    setShowMentionMenu(false);

    setTimeout(() => {
      if (textareaRef.current) {
        textareaRef.current.focus();
        const nextPos = replacedBefore.length;
        textareaRef.current.setSelectionRange(nextPos, nextPos);
      }
    }, 10);
  };

  const handleSend = async () => {
    if (!input.trim() || !activeProjectId) return;
    
    setIsSending(true);

    // Continuous Scheduled Automations: /schedule <interval/cron> <prompt>
    if (input.trim().startsWith('/schedule ')) {
      const rest = input.trim().slice(10).trim();
      let cronExpr = '';
      let promptText = '';
      
      if (rest.startsWith('"') || rest.startsWith("'")) {
        const quote = rest[0];
        const endQuote = rest.indexOf(quote, 1);
        if (endQuote !== -1) {
          cronExpr = rest.slice(1, endQuote);
          promptText = rest.slice(endQuote + 1).trim();
        }
      }
      
      if (!cronExpr) {
        const parts = rest.split(/\s+/);
        if (parts.length >= 5) {
          const potentialCron = parts.slice(0, 5);
          const isValidCron = potentialCron.every(field => /^[0-9*\/,\-]+$/.test(field));
          if (isValidCron) {
            cronExpr = potentialCron.join(' ');
            promptText = parts.slice(5).join(' ').trim();
          }
        }
      }

      if (cronExpr && promptText) {
        try {
          await invoke('create_scheduled_task', {
            projectId: activeProjectId,
            conversationId: activeConversationId || 'scheduled-automations',
            targetAgentId: activeDmAgent ? activeDmAgent.id : 'boss',
            cronExpression: cronExpr,
            humanReadable: promptText.slice(0, 40),
            actionPrompt: promptText,
          });
          setInput('');
          showToast('success', `Scheduled Automation Active: ${cronExpr}`);
          setIsSending(false);
          return;
        } catch (err: any) {
          console.error("Failed to create schedule:", err);
          showToast('error', `Failed to create schedule: ${err?.message || err}`);
          setIsSending(false);
          return;
        }
      }
    }

    try {
      const convoId = activeConversationId || activeProjectId;
      const targetChannelId = activeDmAgent 
        ? activeDmAgent.id 
        : (activeConversationId || 'general');

      // Extract all @mentions from message content
      const mentionMatches = input.match(/@([a-zA-Z0-9_-]+)/g);
      const mentions = mentionMatches ? Array.from(new Set(mentionMatches.map(m => m.slice(1)))) : [];

      await invoke('send_message', {
        projectId: activeProjectId,
        conversationId: convoId,
        channelId: targetChannelId,
        content: input.trim(),
        mentions: mentions
      });
      
      setInput('');
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
      }
      setShowMentionMenu(false);
    } catch (e: any) {
      console.error("Failed to send message:", e);
      showToast('error', `Message routing error: ${e?.message || e}`);
    } finally {
      setIsSending(false);
    }
  };

  return (
    <div className="p-4 border-t border-gray-800 bg-bg-surface relative font-mono text-xs">
      {showMentionMenu && (
        <MentionAutocomplete
          query={mentionQuery}
          onSelect={handleSelectMention}
          onClose={() => setShowMentionMenu(false)}
        />
      )}

      <div className="relative flex items-center">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={handleInputChange}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey && !showMentionMenu) {
              e.preventDefault();
              handleSend();
            }
          }}
          disabled={isSending || !activeProjectId}
          placeholder={
            !activeProjectId 
              ? "Select or create a project to start typing..."
              : activeDmAgent
              ? `Message @${activeDmAgent.name} (Direct Message)...`
              : `Message ${channelTitle} (type @ to mention a specialist)...`
          }
          className="w-full bg-black/40 border border-gray-700 rounded-lg pl-3 pr-20 py-2.5 text-xs focus:outline-none focus:border-brand-primary text-gray-200 resize-none disabled:opacity-50"
          rows={1}
        />

        <div className="absolute right-2 flex items-center space-x-1">
          <button
            type="button"
            onClick={() => {
              setInput(prev => prev + '@');
              setShowMentionMenu(true);
              setMentionQuery('');
              textareaRef.current?.focus();
            }}
            className="p-1 text-gray-500 hover:text-gray-300 transition-colors"
            title="Mention an agent"
          >
            <AtSign size={14} />
          </button>

          <button 
            onClick={handleSend}
            disabled={isSending || !activeProjectId || !input.trim()}
            className="p-1.5 rounded bg-brand-primary text-black hover:opacity-90 transition-opacity disabled:opacity-30"
            title="Send Message"
          >
            <SendHorizontal size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
