import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

interface ClipItem {
  id: string;
  content: string;
  source_app: string;
  category: string;
  is_sensitive: bool;
  is_pinned: bool;
  created_at: number;
  paste_count: number;
}

interface PasteLogItem {
  id: number;
  clip_id: string;
  target_app: string;
  pasted_at: number;
}

type bool = boolean;

class ClipzApp {
  private notchShell: HTMLElement;
  private notchHeader: HTMLElement;
  private pillPreview: HTMLElement;
  private streamText: HTMLElement;
  private searchInput: HTMLInputElement;
  private clipsContainer: HTMLElement;
  private emptyState: HTMLElement;
  private clipCount: HTMLElement | null;
  private filterBtns: NodeListOf<HTMLButtonElement>;
  private historyModal: HTMLElement;
  private historyList: HTMLElement;
  private closeModalBtn: HTMLElement;

  private deleteModal: HTMLElement;
  private deleteClipPreview: HTMLElement;
  private closeDeleteModalBtn: HTMLElement;
  private cancelDeleteBtn: HTMLElement;
  private confirmDeleteBtn: HTMLElement;

  private imageLightbox: HTMLElement;
  private lightboxImg: HTMLImageElement;
  private closeLightboxBtn: HTMLElement;

  private searchSection: HTMLElement;
  private timelineSection: HTMLElement;
  private settingsPanel: HTMLElement;
  private settingsBtn: HTMLElement;
  private backFromSettingsBtn: HTMLElement;
  private keycapContainer: HTMLElement;
  private keycapDisplay: HTMLElement;
  private saveIndicator: HTMLElement;
  private resetHotkeyBtn: HTMLElement;
  private osPlatformBadge: HTMLElement;
  private shortcutBadge: HTMLElement;
  private currentShortcut: string = 'Alt+C';
  private isRecordingHotkey: boolean = false;

  private clips: ClipItem[] = [];
  private currentFilter: string = 'all';
  private searchDebounceTimer: any = null;
  private hoverCollapseTimer: any = null;
  private hoverExpandTimer: any = null;
  private previewTimer: any = null;
  private currentState: 'pill' | 'preview' | 'expanded' = 'pill';
  private isExpanded: boolean = false;
  private selectedIndex: number = -1;
  private pendingDeleteId: string | null = null;
  private dragStartTime: number = 0;
  private dragStartX: number = 0;
  private dragStartY: number = 0;

  constructor() {
    this.notchShell = document.getElementById('notch-shell')!;
    this.notchHeader = document.getElementById('notch-header')!;
    this.pillPreview = document.getElementById('pill-preview')!;
    this.streamText = document.getElementById('stream-text')!;
    this.searchInput = document.getElementById('search-input') as HTMLInputElement;
    this.shortcutBadge = document.querySelector('.shortcut-badge') as HTMLElement;
    this.searchSection = document.querySelector('.search-section') as HTMLElement;
    this.timelineSection = document.getElementById('timeline-section')!;
    this.clipsContainer = document.getElementById('clips-container')!;
    this.emptyState = document.getElementById('empty-state')!;
    this.clipCount = document.getElementById('clip-count');
    this.filterBtns = document.querySelectorAll('.filter-btn');
    this.historyModal = document.getElementById('history-modal')!;
    this.historyList = document.getElementById('history-list')!;
    this.closeModalBtn = document.getElementById('close-modal-btn')!;

    this.deleteModal = document.getElementById('delete-modal')!;
    this.deleteClipPreview = document.getElementById('delete-clip-preview')!;
    this.closeDeleteModalBtn = document.getElementById('close-delete-modal-btn')!;
    this.cancelDeleteBtn = document.getElementById('cancel-delete-btn')!;
    this.confirmDeleteBtn = document.getElementById('confirm-delete-btn')!;

    this.imageLightbox = document.getElementById('image-lightbox')!;
    this.lightboxImg = document.getElementById('lightbox-img') as HTMLImageElement;
    this.closeLightboxBtn = document.getElementById('close-lightbox-btn')!;

    this.settingsPanel = document.getElementById('settings-panel')!;
    this.settingsBtn = document.getElementById('settings-btn')!;
    this.backFromSettingsBtn = document.getElementById('back-from-settings-btn')!;
    this.keycapContainer = document.getElementById('keycap-container')!;
    this.keycapDisplay = document.getElementById('keycap-display')!;
    this.saveIndicator = document.getElementById('save-indicator')!;
    this.resetHotkeyBtn = document.getElementById('reset-hotkey-btn')!;
    this.osPlatformBadge = document.getElementById('os-platform-badge')!;

    this.initEventListeners();
    this.loadClips();
    this.initTauriListeners();
    this.loadSettings();

    // Start directly as the ultra-compact micro-pill
    this.setNotchState('pill');
  }

  private collapseWindowTimer: any = null;

  private async setNotchState(state: 'pill' | 'preview' | 'expanded') {
    this.currentState = state;
    clearTimeout(this.previewTimer);
    clearTimeout(this.collapseWindowTimer);

    switch (state) {
      case 'pill':
        this.isExpanded = false;
        this.closeSettingsPanel();
        void this.notchShell.offsetWidth;
        this.notchShell.classList.remove('state-preview', 'state-expanded', 'expanded');
        this.notchShell.classList.add('state-pill', 'collapsed');
        this.searchInput.blur();
        // Wait 420ms for CSS transition (350ms) to completely finish before shrinking native OS window
        this.collapseWindowTimer = setTimeout(async () => {
          if (this.currentState === 'pill') {
            try {
              await invoke('shrink_to_pill');
            } catch (_) {}
          }
        }, 420);
        break;

      case 'preview':
        this.isExpanded = false;
        try {
          await invoke('show_preview_notch');
        } catch (_) {}
        void this.notchShell.offsetWidth;
        requestAnimationFrame(() => {
          this.notchShell.classList.remove('state-pill', 'state-expanded', 'expanded');
          this.notchShell.classList.add('state-preview', 'collapsed');
        });
        break;

      case 'expanded':
        this.isExpanded = true;
        try {
          await invoke('expand_window');
        } catch (_) {}
        void this.notchShell.offsetWidth;
        requestAnimationFrame(() => {
          this.notchShell.classList.remove('state-pill', 'state-preview', 'collapsed');
          this.notchShell.classList.add('state-expanded', 'expanded');
          setTimeout(() => {
            if (this.currentState === 'expanded') {
              this.searchInput.focus({ preventScroll: true });
            }
          }, 120);
        });
        break;
    }
  }

  private initEventListeners() {
    this.notchHeader.addEventListener('mousedown', async (e) => {
      // Don't trigger window drag if clicking action buttons or filter elements
      if ((e.target as HTMLElement).closest('button, input, a, .icon-btn, .filter-btn')) return;
      if (e.button === 0) {
        this.dragStartTime = Date.now();
        this.dragStartX = e.screenX;
        this.dragStartY = e.screenY;
        try {
          await invoke('start_dragging');
        } catch (_) {
          try {
            await getCurrentWindow().startDragging();
          } catch (_) {}
        }
      }
    });

    this.notchHeader.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('button, input, a, .icon-btn, .filter-btn')) return;
      // If mouse moved or was held for drag gesture, do not toggle expand
      const dragDuration = Date.now() - this.dragStartTime;
      const moveDist = Math.hypot(e.screenX - this.dragStartX, e.screenY - this.dragStartY);
      if (dragDuration > 200 || moveDist > 5) {
        return;
      }
      this.toggleExpand();
    });

    const toggleExpandBtn = document.getElementById('toggle-expand-btn');
    if (toggleExpandBtn) {
      toggleExpandBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleExpand();
      });
    }

    // Cancel any pending collapse on mouse enter
    this.notchShell.addEventListener('mouseenter', () => {
      clearTimeout(this.hoverCollapseTimer);
    });

    // Retract notch smoothly back to micro-pill when cursor leaves the Clipz window
    this.notchShell.addEventListener('mouseleave', () => {
      clearTimeout(this.hoverCollapseTimer);
      this.hoverCollapseTimer = setTimeout(() => {
        if (this.currentState === 'expanded' && this.historyModal.classList.contains('hidden')) {
          this.setNotchState('pill');
        }
      }, 350);
    });

    // Collapse back to micro-pill when expanded window loses focus (e.g. clicking background anywhere on screen or another app)
    window.addEventListener('blur', () => {
      if (this.currentState === 'expanded') {
        this.setNotchState('pill');
      }
    });

    // Collapse back to micro-pill when clicking on background outside notch shell
    document.addEventListener('click', (e: MouseEvent) => {
      if (!this.notchShell.contains(e.target as Node) && this.currentState === 'expanded') {
        this.setNotchState('pill');
      }
    });

    // Settings Panel Event Listeners
    if (this.settingsBtn) {
      this.settingsBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.openSettingsPanel();
      });
    }
    if (this.backFromSettingsBtn) {
      this.backFromSettingsBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.closeSettingsPanel();
      });
    }
    if (this.keycapContainer) {
      this.keycapContainer.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleRecordHotkey();
      });
    }
    if (this.resetHotkeyBtn) {
      this.resetHotkeyBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.resetDefaultHotkey();
      });
    }
    const clearHotkeyBtn = document.getElementById('clear-hotkey-btn');
    if (clearHotkeyBtn) {
      clearHotkeyBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.saveHotkey('');
      });
    }
    const saveHotkeyBtn = document.getElementById('save-hotkey-btn');
    if (saveHotkeyBtn) {
      saveHotkeyBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (this.tempRecordedShortcut) {
          this.saveHotkey(this.tempRecordedShortcut);
        }
        this.stopRecordingHotkey();
      });
    }
    const cancelHotkeyBtn = document.getElementById('cancel-hotkey-btn');
    if (cancelHotkeyBtn) {
      cancelHotkeyBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.stopRecordingHotkey();
      });
    }
    const centerNotchBtn = document.getElementById('center-notch-btn');
    if (centerNotchBtn) {
      centerNotchBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        try {
          await invoke('center_window');
        } catch (err) {
          console.error('Failed to center window:', err);
        }
      });
    }

    // Search input handler with FTS5 debounce
    this.searchInput.addEventListener('input', () => {
      clearTimeout(this.searchDebounceTimer);
      this.searchDebounceTimer = setTimeout(() => {
        this.performSearch();
      }, 150);
    });

    // Category Filter Pills
    this.filterBtns.forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.filterBtns.forEach((b) => b.classList.remove('active'));
        btn.classList.add('active');
        this.currentFilter = btn.dataset.filter || 'all';
        this.renderClips();
      });
    });

    // Keyboard Shortcuts Navigation
    window.addEventListener('keydown', (e: KeyboardEvent) => {
      if (this.isRecordingHotkey) {
        this.handleHotkeyKeydown(e);
        return;
      }
      if (e.key === 'Escape') {
        if (!this.settingsPanel.classList.contains('hidden')) {
          this.closeSettingsPanel();
        } else {
          this.setNotchState('pill');
        }
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        this.navigateSelection(1);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        this.navigateSelection(-1);
      } else if (e.key === 'Enter' && this.selectedIndex >= 0) {
        e.preventDefault();
        const visibleClips = this.getFilteredClips();
        const targetClip = visibleClips[this.selectedIndex];
        if (targetClip) {
          const cardEl = this.clipsContainer.querySelector(`[data-id="${targetClip.id}"]`);
          const copyBtnEl = cardEl ? (cardEl.querySelector('.copy-btn') as HTMLButtonElement) : undefined;
          this.copyClip(targetClip, false, copyBtnEl);
        }
      }
    });
  }

  private async initTauriListeners() {
    try {
      // Listen for real-time clipboard captures
      await listen<ClipItem>('new-clip', (event) => {
        const item = event.payload;
        // Filter out existing clip with matching ID to prevent duplicate items
        this.clips = this.clips.filter((c) => c.id !== item.id);
        this.clips.unshift(item);
        this.updatePillPreview(item);
        this.renderClips();

        // Elongate to preview stream showing what was copied, then auto-retract to micro-pill after 3.2s
        if (this.currentState !== 'expanded') {
          this.setNotchState('preview');
          this.notchShell.classList.add('copy-pulse');
          setTimeout(() => this.notchShell.classList.remove('copy-pulse'), 1200);

          clearTimeout(this.previewTimer);
          this.previewTimer = setTimeout(() => {
            if (this.currentState === 'preview') {
              this.setNotchState('pill');
            }
          }, 3200);
        }
      });

      // Listen for global Alt + C hotkey
      await listen('toggle-notch-hotkey', () => {
        this.toggleExpand();
      });

      // Listen for window blur event from Tauri native window manager
      await listen('tauri://blur', () => {
        if (this.currentState === 'expanded') {
          this.setNotchState('pill');
        }
      });

      // Listen for paste tracking events
      await listen<{ target_app: string }>('paste-event', (event) => {
        this.streamText.textContent = `Pasted into ${event.payload.target_app}`;
        this.loadClips();
        setTimeout(() => {
          if (this.clips[0]) {
            this.updatePillPreview(this.clips[0]);
          }
        }, 3000);
      });
    } catch (err) {
      console.warn('Tauri event listeners fallback:', err);
    }
  }

  private async loadClips() {
    try {
      const result = await invoke<ClipItem[]>('get_clips', { limit: 50 });
      this.clips = result || [];
      if (this.clips.length > 0) {
        this.updatePillPreview(this.clips[0]);
      }
      this.renderClips();
    } catch (err) {
      console.error('Failed to load clips:', err);
    }
  }

  private async loadSettings() {
    try {
      const isMac = /Mac|iPod|iPhone|iPad/.test(navigator.platform) || /Mac/.test(navigator.userAgent);
      if (this.osPlatformBadge) {
        if (isMac) {
          this.osPlatformBadge.innerHTML = `<svg width="11" height="11" viewBox="0 0 170 170" fill="#ffffff" style="margin-right:5px;"><path d="M150.37 130.25c-2.45 5.66-5.35 10.87-8.71 15.66-4.58 6.53-8.33 11.05-11.22 13.56-4.48 4.12-9.28 6.23-14.42 6.35-3.69 0-8.14-1.05-13.32-3.18-5.19-2.12-9.97-3.17-14.34-3.17-4.58 0-9.49 1.05-14.75 3.17-5.26 2.13-9.5 3.24-12.74 3.35-4.34.13-9.16-1.9-14.49-6.1-3.26-2.64-7.14-7.27-11.64-13.9-6.85-10.09-12.35-21.2-16.5-33.32-4.14-12.13-6.22-23.75-6.22-34.86 0-14.54 3.73-26.65 11.19-36.33 7.46-9.68 16.73-14.65 27.81-14.9 5.35 0 11.05 1.34 17.1 4.02 6.05 2.68 10.3 4.02 12.74 4.02 2.18 0 6.46-1.39 12.86-4.17 6.4-2.78 11.83-4.04 16.29-3.78 12.01.63 21.7 5.1 29.08 13.41-10.74 6.53-15.99 15.54-15.74 27.03.25 9.05 3.76 16.59 10.53 22.63 6.77 6.04 14.88 9.53 24.32 10.48-2.52 7.42-5.91 14.85-10.18 22.31zM119.22 31.09c0-6.79 2.5-13.34 7.51-19.65 5.01-6.31 11.31-10.34 18.9-12.09.25 1.13.38 2.14.38 3.02 0 6.66-2.56 13.19-7.68 19.58-5.12 6.4-11.41 10.45-18.87 12.16-.06-1.01-.24-2.02-.24-3.02z"/></svg><span style="color:#ffffff;">macOS</span>`;
        } else {
          this.osPlatformBadge.innerHTML = `<svg width="11" height="11" viewBox="0 0 88 88" style="margin-right:5px;"><path d="M0 12.402l35.687-4.86.016 34.423-35.67.202zm35.67 33.329l.029 34.502L0 75.312V45.928zm4.357-38.932L88 0v41.442l-47.973.473zm47.973 39.467V88L40.027 81.258l-.017-34.786z" fill="#0078D4"/></svg><span style="color:#0078D4;">Windows</span>`;
        }
      }

      const shortcut = await invoke<string>('get_global_shortcut');
      const activeShortcut = shortcut || (isMac ? 'Cmd+Shift+C' : 'Alt+C');
      this.currentShortcut = activeShortcut;
      this.renderKeycaps(activeShortcut);

      if (this.shortcutBadge) {
        this.shortcutBadge.textContent = activeShortcut;
      }
    } catch (err) {
      console.warn('Failed to load settings:', err);
    }
  }

  private renderKeycaps(shortcutStr: string) {
    if (!this.keycapDisplay) return;

    if (this.isRecordingHotkey) {
      if (this.keycapContainer) this.keycapContainer.classList.add('recording');
      this.keycapDisplay.innerHTML = `Press shortcut keys...`;
      return;
    }

    if (this.keycapContainer) this.keycapContainer.classList.remove('recording');

    if (!shortcutStr || shortcutStr.trim() === '') {
      this.keycapDisplay.innerHTML = `<span style="color: var(--text-dim); font-style: italic;">None</span>`;
      return;
    }

    const parts = shortcutStr.split('+');
    const formatted = parts
      .map((part) => {
        let label = part.trim();
        if (label === 'Cmd' || label === 'Meta') label = '⌘';
        else if (label === 'Alt') label = 'Alt';
        else if (label === 'Shift') label = 'Shift';
        else if (label === 'Ctrl' || label === 'Control') label = 'Ctrl';
        return label;
      })
      .join(' + ');

    this.keycapDisplay.innerHTML = formatted;
  }

  private openSettingsPanel() {
    this.searchSection.classList.add('hidden');
    this.timelineSection.classList.add('hidden');
    const footer = document.getElementById('notch-footer');
    if (footer) footer.classList.add('hidden');
    this.settingsPanel.classList.remove('hidden');
    this.loadSettings();
  }

  private closeSettingsPanel() {
    this.stopRecordingHotkey();
    this.settingsPanel.classList.add('hidden');
    this.searchSection.classList.remove('hidden');
    this.timelineSection.classList.remove('hidden');
    const footer = document.getElementById('notch-footer');
    if (footer) footer.classList.remove('hidden');
  }

  private tempRecordedShortcut: string = '';

  private toggleRecordHotkey() {
    if (this.isRecordingHotkey) {
      this.stopRecordingHotkey();
    } else {
      this.startRecordingHotkey();
    }
  }

  private startRecordingHotkey() {
    this.isRecordingHotkey = true;
    this.tempRecordedShortcut = '';
    const popover = document.getElementById('recording-popover');
    const statusText = document.getElementById('recording-status-text');
    const box = document.querySelector('.recording-display-box');
    if (popover) popover.classList.remove('hidden');
    if (statusText) statusText.innerText = 'Type on your keyboard';
    if (box) box.classList.remove('has-keys');
    if (this.keycapContainer) this.keycapContainer.classList.add('recording');
  }

  private stopRecordingHotkey() {
    this.isRecordingHotkey = false;
    this.tempRecordedShortcut = '';
    const popover = document.getElementById('recording-popover');
    if (popover) popover.classList.add('hidden');
    if (this.keycapContainer) this.keycapContainer.classList.remove('recording');
    this.renderKeycaps(this.currentShortcut);
  }

  private handleHotkeyKeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    if (e.key === 'Escape') {
      this.stopRecordingHotkey();
      return;
    }

    if (e.key === 'Enter') {
      if (this.tempRecordedShortcut) {
        this.saveHotkey(this.tempRecordedShortcut);
      }
      this.stopRecordingHotkey();
      return;
    }

    const parts: string[] = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.metaKey) parts.push('Cmd');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');

    let keyName = e.key;
    const statusText = document.getElementById('recording-status-text');
    const box = document.querySelector('.recording-display-box');

    if (['Control', 'Alt', 'Shift', 'Meta'].includes(keyName)) {
      const displayStr = parts.map(p => p === 'Cmd' || p === 'Meta' ? '⌘' : p).join(' + ');
      if (statusText) statusText.innerText = displayStr || 'Type on your keyboard';
      if (box) box.classList.remove('has-keys');
      return;
    }

    if (keyName === ' ') keyName = 'Space';
    else if (keyName.length === 1) keyName = keyName.toUpperCase();

    parts.push(keyName);
    this.tempRecordedShortcut = parts.join('+');

    const formattedDisplay = parts.map(p => p === 'Cmd' || p === 'Meta' ? '⌘' : p).join(' + ');
    if (statusText) statusText.innerText = formattedDisplay;
    if (box) box.classList.add('has-keys');
  }

  private async resetDefaultHotkey() {
    const isMac = /Mac|iPod|iPhone|iPad/.test(navigator.platform) || /Mac/.test(navigator.userAgent);
    const defaultShortcut = isMac ? 'Cmd+Shift+C' : 'Alt+C';
    await this.saveHotkey(defaultShortcut);
  }

  private async saveHotkey(shortcut: string) {
    if (!shortcut) return;
    try {
      this.currentShortcut = shortcut;
      this.isRecordingHotkey = false;
      this.renderKeycaps(shortcut);

      await invoke('save_global_shortcut', { shortcut });
      if (this.shortcutBadge) {
        this.shortcutBadge.textContent = shortcut;
      }

      if (this.saveIndicator) {
        this.saveIndicator.classList.remove('hidden');
        setTimeout(() => {
          this.saveIndicator.classList.add('hidden');
        }, 1500);
      }
    } catch (err) {
      console.error('Failed to save shortcut:', err);
    }
  }

  private async performSearch() {
    const query = this.searchInput.value.trim();
    try {
      const result = await invoke<ClipItem[]>('search_clips', { query });
      this.clips = result || [];
      this.renderClips();
    } catch (err) {
      console.error('Failed to search clips:', err);
    }
  }

  private updatePillPreview(item: ClipItem) {
    if (item.is_sensitive) {
      this.streamText.innerHTML = `<span class="green-check">✓</span> 🔒 Sensitive data captured from ${this.escapeHTML(item.source_app)}`;
    } else if (item.category === 'image') {
      const cleanApp = item.source_app && item.source_app !== 'Unknown App'
        ? item.source_app.replace(/\.exe$/i, '')
        : 'Screenshot';
      const date = new Date(item.created_at * 1000);
      const timeStr = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      this.streamText.innerHTML = `<span class="green-check">✓</span> 🖼️ Screenshot captured from ${this.escapeHTML(cleanApp)} (${timeStr})`;
    } else {
      const preview = item.content.length > 40 ? item.content.slice(0, 40) + '...' : item.content;
      this.streamText.innerHTML = `<span class="green-check">✓</span> ${this.escapeHTML(item.source_app)}: "${this.escapeHTML(preview)}"`;
    }
  }

  private toggleExpand() {
    if (this.currentState === 'expanded') {
      this.setNotchState('pill');
    } else {
      this.setNotchState('expanded');
    }
  }

  private async expandNotch() {
    this.setNotchState('expanded');
  }

  private async collapseNotch() {
    this.setNotchState('pill');
  }

  private getFilteredClips(): ClipItem[] {
    if (this.currentFilter === 'all') return this.clips;
    if (this.currentFilter === 'sensitive') return this.clips.filter((c) => c.is_sensitive || c.category === 'sensitive');
    return this.clips.filter((c) => c.category === this.currentFilter);
  }

  private renderClips() {
    const filtered = this.getFilteredClips();
    if (this.clipCount) {
      this.clipCount.textContent = `${this.clips.length} items stored`;
    }

    if (filtered.length === 0) {
      this.emptyState.style.display = 'flex';
      this.clipsContainer.innerHTML = '';
      return;
    }

    this.emptyState.style.display = 'none';
    this.clipsContainer.innerHTML = filtered
      .map((clip, index) => this.createClipCardHTML(clip, index))
      .join('');

    this.bindCardEvents();
  }

  private getCategoryIconHTML(clip: ClipItem): string {
    if (clip.is_sensitive) {
      return `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#f59e0b" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>`;
    }
    if (clip.category === 'image') {
      const src = clip.content.startsWith('data:image/') ? clip.content : `data:image/png;base64,${clip.content}`;
      return `<img src="${src}" class="clip-thumb-img" alt="Thumbnail" />`;
    }
    switch (clip.category) {
      case 'code':
        return `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#60a5fa" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>`;
      case 'link':
        return `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#38bdf8" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>`;
      case 'text':
      default:
        return `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#9ca3af" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 7 4 4 20 4 20 7"/><line x1="12" y1="4" x2="12" y2="20"/><line x1="9" y1="20" x2="15" y2="20"/></svg>`;
    }
  }

  private renderClipPreviewText(clip: ClipItem): string {
    const rawContent = clip.content || '';
    const singleLine = rawContent.replace(/\r?\n/g, ' ').replace(/\s+/g, ' ').trim();

    if (clip.is_sensitive) {
      const truncated = singleLine.length > 30 ? singleLine.slice(0, 30) + '...' : singleLine;
      return `<span class="sensitive-text">•••••••••••• ${this.escapeHTML(truncated || 'API key')}</span>`;
    }
    if (clip.category === 'image') {
      const cleanApp = clip.source_app && clip.source_app !== 'Unknown App'
        ? clip.source_app.replace(/\.exe$/i, '')
        : 'Screenshot';
      const date = new Date(clip.created_at * 1000);
      const year = date.getFullYear();
      const month = String(date.getMonth() + 1).padStart(2, '0');
      const day = String(date.getDate()).padStart(2, '0');
      const hours = String(date.getHours()).padStart(2, '0');
      const mins = String(date.getMinutes()).padStart(2, '0');
      const secs = String(date.getSeconds()).padStart(2, '0');
      const imageName = `Screenshot_${cleanApp}_${year}${month}${day}_${hours}${mins}${secs}.png`;
      return `<span class="image-text">🖼️ ${this.escapeHTML(imageName)}</span>`;
    }
    if (clip.category === 'link') {
      return `<span class="link-text">${this.escapeHTML(singleLine)}</span>`;
    }
    return this.escapeHTML(singleLine || '(empty clip)');
  }

  private createClipCardHTML(clip: ClipItem, index: number): string {
    const isSelected = index === this.selectedIndex;
    const timeAgo = this.formatTimeAgo(clip.created_at);
    const iconHTML = this.getCategoryIconHTML(clip);
    const contentHTML = this.renderClipPreviewText(clip);

    return `
      <div class="clip-card ${clip.is_pinned ? 'pinned' : ''} ${isSelected ? 'selected' : ''}" data-id="${clip.id}">
        <div class="clip-icon">
          ${iconHTML}
        </div>
        <div class="clip-body">
          <div class="clip-main-text ${clip.category === 'code' ? 'code-font' : ''}">
            ${contentHTML}
          </div>
          <div class="clip-sub-meta">
            <span>${this.escapeHTML(clip.source_app)}</span>
            <span class="meta-dot">•</span>
            <span>${timeAgo}</span>
            ${clip.paste_count > 0 ? `<span class="meta-dot">•</span><span>Pasted ${clip.paste_count}x</span>` : ''}
          </div>
        </div>
        <div class="clip-right-actions">
          ${isSelected ? '<span class="enter-badge" title="Press Enter to Copy">↵</span>' : ''}
          <div class="action-btn-group">
            <button class="action-btn info-btn" title="View Paste History">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
            </button>
            <button class="action-btn copy-btn" title="Copy to Clipboard">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
            <button class="action-btn delete-btn" title="Delete Clip">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private bindCardEvents() {
    const cards = this.clipsContainer.querySelectorAll('.clip-card');
    cards.forEach((card) => {
      const id = card.getAttribute('data-id')!;
      const clip = this.clips.find((c) => c.id === id);

      const infoBtn = card.querySelector('.info-btn');
      const copyBtn = card.querySelector('.copy-btn');
      const deleteBtn = card.querySelector('.delete-btn');
      const revealBtn = card.querySelector('.reveal-btn');

      if (infoBtn) {
        infoBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.stopImmediatePropagation();
          this.showPasteHistory(id);
        });
      }

      if (copyBtn) {
        copyBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.stopImmediatePropagation();
          if (clip) this.copyClip(clip, false, copyBtn as HTMLButtonElement);
        });
      }

      if (deleteBtn) {
        deleteBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.stopImmediatePropagation();
          this.promptDelete(id);
        });
      }

      if (revealBtn) {
        revealBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.stopImmediatePropagation();
          this.revealSensitive(id, revealBtn as HTMLButtonElement);
        });
      }

      card.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        if (target.closest('.action-btn') || target.closest('.reveal-btn')) {
          return;
        }
        if (clip && clip.category === 'image') {
          this.openLightbox(clip.content);
        } else if (clip) {
          const isExpanded = card.classList.toggle('expanded-text');
          const mainTextEl = card.querySelector('.clip-main-text');
          if (mainTextEl) {
            if (isExpanded) {
              if (clip.is_sensitive) {
                mainTextEl.innerHTML = `<span class="sensitive-text">🔒 ${this.escapeHTML(clip.content)}</span>`;
              } else if (clip.category === 'link') {
                mainTextEl.innerHTML = `<span class="link-text">${this.escapeHTML(clip.content)}</span>`;
              } else {
                mainTextEl.innerHTML = this.escapeHTML(clip.content);
              }
            } else {
              mainTextEl.innerHTML = this.renderClipPreviewText(clip);
            }
          }
        }
      });
    });

    this.closeModalBtn.onclick = () => {
      this.historyModal.classList.add('hidden');
    };

    this.historyModal.onclick = (e) => {
      if (e.target === this.historyModal) {
        this.historyModal.classList.add('hidden');
      }
    };

    this.closeDeleteModalBtn.onclick = () => {
      this.deleteModal.classList.add('hidden');
    };

    this.cancelDeleteBtn.onclick = () => {
      this.deleteModal.classList.add('hidden');
    };

    this.confirmDeleteBtn.onclick = async () => {
      if (this.pendingDeleteId) {
        await this.deleteClip(this.pendingDeleteId);
        this.deleteModal.classList.add('hidden');
        this.pendingDeleteId = null;
      }
    };

    this.deleteModal.onclick = (e) => {
      if (e.target === this.deleteModal) {
        this.deleteModal.classList.add('hidden');
      }
    };

    this.closeLightboxBtn.onclick = () => {
      this.imageLightbox.classList.add('hidden');
    };

    this.imageLightbox.onclick = (e) => {
      if (e.target === this.imageLightbox) {
        this.imageLightbox.classList.add('hidden');
      }
    };
  }

  private openLightbox(src: string) {
    this.lightboxImg.src = src;
    this.imageLightbox.classList.remove('hidden');
  }

  private promptDelete(id: string) {
    const clip = this.clips.find((c) => c.id === id);
    if (!clip) return;

    this.pendingDeleteId = id;
    if (clip.category === 'image') {
      this.deleteClipPreview.innerHTML = `<img src="${clip.content}" style="max-height:65px; border-radius:6px;" alt="Clip Image"/>`;
    } else if (clip.is_sensitive) {
      this.deleteClipPreview.textContent = '🔒 Password Protected Clip';
    } else {
      this.deleteClipPreview.textContent = clip.content.length > 100 ? clip.content.slice(0, 100) + '...' : clip.content;
    }
    this.deleteModal.classList.remove('hidden');
  }

  private async copyClip(clip: ClipItem, autoPaste: boolean = false, btnElement?: HTMLButtonElement) {
    try {
      if (btnElement) {
        const origHTML = btnElement.innerHTML;
        btnElement.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#10b981" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;
        btnElement.classList.add('copied-success');
        setTimeout(() => {
          btnElement.innerHTML = origHTML;
          btnElement.classList.remove('copied-success');
        }, 1800);
      }

      await invoke('copy_to_clipboard', {
        id: clip.id,
        content: clip.content,
        auto_paste: autoPaste,
      });
      this.streamText.innerHTML = `<span class="green-check">✓</span> ${autoPaste ? 'Pasted into active app!' : 'Copied to clipboard!'}`;
      setTimeout(() => this.updatePillPreview(clip), 2000);
    } catch (err) {
      console.error('Failed to copy clip:', err);
    }
  }

  private async showPasteHistory(id: string) {
    try {
      const logs = await invoke<PasteLogItem[]>('get_paste_history', { id });
      if (!logs || logs.length === 0) {
        this.historyList.innerHTML = `
          <div class="empty-history">
            <p>No paste events recorded yet for this clip.</p>
            <span>Paste using Ctrl+V or click to auto-paste.</span>
          </div>
        `;
      } else {
        this.historyList.innerHTML = logs
          .map(
            (log) => `
            <div class="history-item">
              <div class="history-app">
                <span class="app-badge">${this.escapeHTML(log.target_app)}</span>
                <span class="history-action">Pasted into application</span>
              </div>
              <span class="history-time">${this.formatTimeAgo(log.pasted_at)}</span>
            </div>
          `
          )
          .join('');
      }
      this.historyModal.classList.remove('hidden');
    } catch (err) {
      console.error('Failed to fetch paste history:', err);
    }
  }

  private async togglePin(id: string) {
    try {
      const newPinnedState = await invoke<boolean>('toggle_pin', { id });
      const clip = this.clips.find((c) => c.id === id);
      if (clip) clip.is_pinned = newPinnedState;
      this.renderClips();
    } catch (err) {
      console.error('Failed to toggle pin:', err);
    }
  }

  private async deleteClip(id: string) {
    // Optimistically update UI state immediately
    this.clips = this.clips.filter((c) => c.id !== id);
    this.renderClips();
    this.streamText.textContent = `Clip deleted permanently.`;

    try {
      await invoke('delete_clip', { id });
    } catch (err) {
      console.error('Failed to delete clip from DB:', err);
      await this.loadClips();
    }
  }

  private async revealSensitive(id: string, btnElement: HTMLButtonElement) {
    try {
      const plainText = await invoke<string>('reveal_sensitive', { id });
      const cardContent = btnElement.parentElement;
      if (cardContent) {
        cardContent.innerHTML = `<span class="clip-content code-font">${this.escapeHTML(plainText)}</span>`;
        // Auto mask after 10 seconds
        setTimeout(() => {
          this.renderClips();
        }, 10000);
      }
    } catch (err) {
      btnElement.textContent = 'Expired from RAM';
      btnElement.disabled = true;
    }
  }

  private navigateSelection(direction: number) {
    const filtered = this.getFilteredClips();
    if (filtered.length === 0) return;

    this.selectedIndex += direction;
    if (this.selectedIndex < 0) this.selectedIndex = 0;
    if (this.selectedIndex >= filtered.length) this.selectedIndex = filtered.length - 1;

    this.renderClips();
  }

  private formatTimeAgo(timestampSecs: number): string {
    const now = Math.floor(Date.now() / 1000);
    const diff = now - timestampSecs;
    if (diff < 60) return 'Just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  }

  private escapeHTML(str: string): string {
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }
}

document.addEventListener('DOMContentLoaded', () => {
  new ClipzApp();
});
