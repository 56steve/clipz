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

  private clips: ClipItem[] = [];
  private currentFilter: string = 'all';
  private searchDebounceTimer: any = null;
  private hoverCollapseTimer: any = null;
  private hoverExpandTimer: any = null;
  private isExpanded: boolean = false;
  private selectedIndex: number = -1;
  private pendingDeleteId: string | null = null;

  constructor() {
    this.notchShell = document.getElementById('notch-shell')!;
    this.notchHeader = document.getElementById('notch-header')!;
    this.pillPreview = document.getElementById('pill-preview')!;
    this.streamText = document.getElementById('stream-text')!;
    this.searchInput = document.getElementById('search-input') as HTMLInputElement;
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

    this.initEventListeners();
    this.loadClips();
    this.initTauriListeners();
  }

  private initEventListeners() {
    this.notchHeader.addEventListener('click', () => {
      this.toggleExpand();
    });

    // Smart Hover Dwell: 180ms delay to prevent accidental expand when moving mouse across top bar
    this.notchShell.addEventListener('mouseenter', () => {
      clearTimeout(this.hoverCollapseTimer);
      clearTimeout(this.hoverExpandTimer);
      this.hoverExpandTimer = setTimeout(() => {
        if (!this.isExpanded) {
          this.expandNotch();
        }
      }, 180);
    });

    // Retract notch smoothly when cursor leaves the Clipz window
    this.notchShell.addEventListener('mouseleave', () => {
      clearTimeout(this.hoverExpandTimer);
      clearTimeout(this.hoverCollapseTimer);
      this.hoverCollapseTimer = setTimeout(() => {
        if (this.isExpanded && this.historyModal.classList.contains('hidden')) {
          this.collapseNotch();
        }
      }, 300);
    });

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
      if (e.key === 'Escape') {
        this.collapseNotch();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        this.navigateSelection(1);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        this.navigateSelection(-1);
      } else if (e.key === 'Enter' && this.selectedIndex >= 0) {
        e.preventDefault();
        const visibleClips = this.getFilteredClips();
        if (visibleClips[this.selectedIndex]) {
          this.copyClip(visibleClips[this.selectedIndex]);
        }
      }
    });
  }

  private async initTauriListeners() {
    try {
      // Listen for real-time clipboard captures from Win32 backend stream
      await listen<ClipItem>('new-clip', (event) => {
        const item = event.payload;
        this.clips.unshift(item);
        this.updatePillPreview(item);
        this.renderClips();

        // 1.2s Copy Pulse Feedback Badge
        this.notchShell.classList.add('copy-pulse');
        setTimeout(() => this.notchShell.classList.remove('copy-pulse'), 1200);
      });

      // Listen for global Alt + C hotkey
      await listen('toggle-notch-hotkey', () => {
        this.toggleExpand();
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
      this.streamText.textContent = `🔒 Sensitive data captured from ${item.source_app}`;
    } else if (item.category === 'image') {
      const cleanApp = item.source_app && item.source_app !== 'Unknown App'
        ? item.source_app.replace(/\.exe$/i, '')
        : 'Screenshot';
      const date = new Date(item.created_at * 1000);
      const timeStr = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      this.streamText.textContent = `🖼️ Screenshot captured from ${cleanApp} (${timeStr})`;
    } else {
      const preview = item.content.length > 40 ? item.content.slice(0, 40) + '...' : item.content;
      this.streamText.textContent = `${item.source_app}: "${preview}"`;
    }
  }

  private toggleExpand() {
    if (this.isExpanded) {
      this.collapseNotch();
    } else {
      this.expandNotch();
    }
  }

  private async expandNotch() {
    if (this.isExpanded) return;
    this.isExpanded = true;
    try {
      await invoke('expand_window');
    } catch (_) {}
    setTimeout(() => {
      if (this.isExpanded) {
        this.notchShell.classList.remove('collapsed');
        this.notchShell.classList.add('expanded');
        this.searchInput.focus();
      }
    }, 30);
  }

  private async collapseNotch() {
    if (!this.isExpanded) return;
    this.isExpanded = false;
    this.notchShell.classList.remove('expanded');
    this.notchShell.classList.add('collapsed');
    this.searchInput.blur();
    try {
      await invoke('collapse_window');
    } catch (_) {}
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
          if (clip) this.copyClip(clip, false);
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

  private async copyClip(clip: ClipItem, autoPaste: boolean = false) {
    try {
      await invoke('copy_to_clipboard', {
        id: clip.id,
        content: clip.content,
        auto_paste: autoPaste,
      });
      this.streamText.textContent = autoPaste ? `Pasted into active app!` : `Copied to clipboard!`;
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
