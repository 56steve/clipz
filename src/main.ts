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

type bool = boolean;

class ClipzApp {
  private notchShell: HTMLElement;
  private notchHeader: HTMLElement;
  private pillPreview: HTMLElement;
  private streamText: HTMLElement;
  private searchInput: HTMLInputElement;
  private clipsContainer: HTMLElement;
  private emptyState: HTMLElement;
  private clipCount: HTMLElement;
  private filterBtns: NodeListOf<HTMLButtonElement>;

  private clips: ClipItem[] = [];
  private currentFilter: string = 'all';
  private searchDebounceTimer: any = null;
  private isExpanded: boolean = false;
  private selectedIndex: number = -1;

  constructor() {
    this.notchShell = document.getElementById('notch-shell')!;
    this.notchHeader = document.getElementById('notch-header')!;
    this.pillPreview = document.getElementById('pill-preview')!;
    this.streamText = document.getElementById('stream-text')!;
    this.searchInput = document.getElementById('search-input') as HTMLInputElement;
    this.clipsContainer = document.getElementById('clips-container')!;
    this.emptyState = document.getElementById('empty-state')!;
    this.clipCount = document.getElementById('clip-count')!;
    this.filterBtns = document.querySelectorAll('.filter-btn');

    this.initEventListeners();
    this.loadClips();
    this.initTauriListeners();
  }

  private initEventListeners() {
    // Header click toggles expand/collapse
    this.notchHeader.addEventListener('click', () => {
      this.toggleExpand();
    });

    // Auto expand on hover over top notch header
    this.notchShell.addEventListener('mouseenter', () => {
      if (!this.isExpanded) {
        this.expandNotch();
      }
    });

    // Collapse when mouse leaves notch container
    this.notchShell.addEventListener('mouseleave', () => {
      if (this.isExpanded && !this.searchInput.value.trim()) {
        this.collapseNotch();
      }
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
      });

      // Listen for paste tracking events
      await listen<{ target_app: string }>('paste-event', (event) => {
        this.streamText.textContent = `Pasted into ${event.payload.target_app}`;
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
    this.isExpanded = true;
    try {
      await getCurrentWindow().setSize(new LogicalSize(700, 500));
    } catch (_) {}
    this.notchShell.classList.remove('collapsed');
    this.notchShell.classList.add('expanded');
    setTimeout(() => this.searchInput.focus(), 100);
  }

  private async collapseNotch() {
    this.isExpanded = false;
    this.notchShell.classList.remove('expanded');
    this.notchShell.classList.add('collapsed');
    this.searchInput.blur();
    try {
      await getCurrentWindow().setSize(new LogicalSize(700, 48));
    } catch (_) {}
  }

  private getFilteredClips(): ClipItem[] {
    if (this.currentFilter === 'all') return this.clips;
    return this.clips.filter((c) => c.category === this.currentFilter);
  }

  private renderClips() {
    const filtered = this.getFilteredClips();
    this.clipCount.textContent = `${this.clips.length} items stored`;

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

  private createClipCardHTML(clip: ClipItem, index: number): string {
    const isSelected = index === this.selectedIndex;
    const timeAgo = this.formatTimeAgo(clip.created_at);

    if (clip.is_sensitive) {
      return `
        <div class="clip-card sensitive-card ${clip.is_pinned ? 'pinned' : ''} ${isSelected ? 'selected' : ''}" data-id="${clip.id}">
          <div class="clip-card-header">
            <div class="app-meta">
              <span class="app-badge">${this.escapeHTML(clip.source_app)}</span>
              <span>${timeAgo}</span>
            </div>
            <div class="card-actions">
              <button class="action-btn pin-btn" title="Pin Clip">${clip.is_pinned ? '📌' : '📍'}</button>
              <button class="action-btn delete-btn" title="Delete Clip">🗑️</button>
            </div>
          </div>
          <div class="sensitive-masked">
            <span>🔒 Password Protected</span>
            <button class="reveal-btn" data-id="${clip.id}">Reveal (10s)</button>
          </div>
        </div>
      `;
    }

    const isCode = clip.category === 'code';
    return `
      <div class="clip-card ${clip.is_pinned ? 'pinned' : ''} ${isSelected ? 'selected' : ''}" data-id="${clip.id}">
        <div class="clip-card-header">
          <div class="app-meta">
            <span class="app-badge">${this.escapeHTML(clip.source_app)}</span>
            <span>${timeAgo}</span>
            ${clip.paste_count > 0 ? `<span>• Pasted ${clip.paste_count}x</span>` : ''}
          </div>
          <div class="card-actions">
            <button class="action-btn pin-btn" title="Pin Clip">${clip.is_pinned ? '📌' : '📍'}</button>
            <button class="action-btn delete-btn" title="Delete Clip">🗑️</button>
          </div>
        </div>
        <div class="clip-content ${isCode ? 'code-font' : ''}">${this.escapeHTML(clip.content)}</div>
      </div>
    `;
  }

  private bindCardEvents() {
    const cards = this.clipsContainer.querySelectorAll('.clip-card');
    cards.forEach((card) => {
      const id = card.getAttribute('data-id')!;
      const clip = this.clips.find((c) => c.id === id);

      card.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        if (target.classList.contains('pin-btn')) {
          e.stopPropagation();
          this.togglePin(id);
        } else if (target.classList.contains('delete-btn')) {
          e.stopPropagation();
          this.deleteClip(id);
        } else if (target.classList.contains('reveal-btn')) {
          e.stopPropagation();
          this.revealSensitive(id, target as HTMLButtonElement);
        } else if (clip) {
          this.copyClip(clip);
        }
      });
    });
  }

  private async copyClip(clip: ClipItem) {
    try {
      await invoke('copy_to_clipboard', {
        id: clip.id,
        content: clip.content,
      });
      this.streamText.textContent = `Copied to clipboard!`;
      setTimeout(() => this.updatePillPreview(clip), 2000);
      this.collapseNotch();
    } catch (err) {
      console.error('Failed to copy clip:', err);
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
    try {
      await invoke('delete_clip', { id });
      this.clips = this.clips.filter((c) => c.id !== id);
      this.renderClips();
    } catch (err) {
      console.error('Failed to delete clip:', err);
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
