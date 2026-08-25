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
  reminder_at?: number | null;
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

  private reminderModal: HTMLElement;
  private reminderClipPreview: HTMLElement;
  private reminderDatetimeInput: HTMLInputElement;
  private closeReminderModalBtn: HTMLElement;
  private cancelReminderBtn: HTMLElement;
  private saveReminderBtn: HTMLElement;
  private clearReminderBtn: HTMLElement;

  private notchRemindersBtn: HTMLElement;
  private remindersListModal: HTMLElement;
  private remindersManagerList: HTMLElement;
  private closeRemindersListModalBtn: HTMLElement;

  private activeReminderClipId: string | null = null;
  private triggeredReminderIds: Set<string> = new Set();
  private reminderCheckInterval: any = null;

  private deleteModal: HTMLElement;
  private deleteClipPreview: HTMLElement;
  private closeDeleteModalBtn: HTMLElement;
  private cancelDeleteBtn: HTMLElement;
  private confirmDeleteBtn: HTMLElement;

  private imageLightbox: HTMLElement;
  private lightboxImg: HTMLImageElement;
  private closeLightboxBtn: HTMLElement;
  private lightboxInfoBtn: HTMLElement;
  private lightboxInfoCard: HTMLElement;
  private closeLightboxInfoBtn: HTMLElement;
  private lbInfoApp: HTMLElement;
  private lbInfoDate: HTMLElement;
  private lbInfoDim: HTMLElement;
  private lbInfoPastes: HTMLElement;
  private activeLightboxClip: ClipItem | null = null;

  private searchSection: HTMLElement;
  private timelineSection: HTMLElement;
  private settingsPanel: HTMLElement;
  private settingsBtn: HTMLElement;
  private backFromSettingsBtn: HTMLElement;
  private keycapContainer: HTMLElement;
  private keycapDisplay: HTMLElement;
  private saveIndicator: HTMLElement;
  private resetHotkeyBtn: HTMLElement;
  private disableHotkeyBtn: HTMLElement;
  private uninstallBtn: HTMLElement;
  private uninstallModal: HTMLElement;
  private closeUninstallModalBtn: HTMLElement;
  private cancelUninstallBtn: HTMLElement;
  private confirmUninstallBtn: HTMLElement;
  private cleanDbCheckbox: HTMLInputElement;
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
  private isDraggingWindow: boolean = false;

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

    this.reminderModal = document.getElementById('reminder-modal')!;
    this.reminderClipPreview = document.getElementById('reminder-clip-preview')!;
    this.reminderDatetimeInput = document.getElementById('reminder-datetime-input') as HTMLInputElement;
    this.closeReminderModalBtn = document.getElementById('close-reminder-modal-btn')!;
    this.cancelReminderBtn = document.getElementById('cancel-reminder-btn')!;
    this.saveReminderBtn = document.getElementById('save-reminder-btn')!;
    this.clearReminderBtn = document.getElementById('clear-reminder-btn')!;

    this.notchRemindersBtn = document.getElementById('notch-reminders-btn')!;
    this.remindersListModal = document.getElementById('reminders-list-modal')!;
    this.remindersManagerList = document.getElementById('reminders-manager-list')!;
    this.closeRemindersListModalBtn = document.getElementById('close-reminders-list-modal-btn')!;

    this.deleteModal = document.getElementById('delete-modal')!;
    this.deleteClipPreview = document.getElementById('delete-clip-preview')!;
    this.closeDeleteModalBtn = document.getElementById('close-delete-modal-btn')!;
    this.cancelDeleteBtn = document.getElementById('cancel-delete-btn')!;
    this.confirmDeleteBtn = document.getElementById('confirm-delete-btn')!;

    this.imageLightbox = document.getElementById('image-lightbox')!;
    this.lightboxImg = document.getElementById('lightbox-img') as HTMLImageElement;
    this.closeLightboxBtn = document.getElementById('close-lightbox-btn')!;
    this.lightboxInfoBtn = document.getElementById('lightbox-info-btn')!;
    this.lightboxInfoCard = document.getElementById('lightbox-info-card')!;
    this.closeLightboxInfoBtn = document.getElementById('close-lightbox-info-btn')!;
    this.lbInfoApp = document.getElementById('lb-info-app')!;
    this.lbInfoDate = document.getElementById('lb-info-date')!;
    this.lbInfoDim = document.getElementById('lb-info-dim')!;
    this.lbInfoPastes = document.getElementById('lb-info-pastes')!;

    this.settingsPanel = document.getElementById('settings-panel')!;
    this.settingsBtn = document.getElementById('settings-btn')!;
    this.backFromSettingsBtn = document.getElementById('back-from-settings-btn')!;
    this.keycapContainer = document.getElementById('keycap-container')!;
    this.keycapDisplay = document.getElementById('keycap-display')!;
    this.saveIndicator = document.getElementById('save-indicator')!;
    this.resetHotkeyBtn = document.getElementById('reset-hotkey-btn')!;
    this.disableHotkeyBtn = document.getElementById('disable-hotkey-btn')!;
    this.uninstallBtn = document.getElementById('uninstall-btn')!;
    this.uninstallModal = document.getElementById('uninstall-modal')!;
    this.closeUninstallModalBtn = document.getElementById('close-uninstall-modal-btn')!;
    this.cancelUninstallBtn = document.getElementById('cancel-uninstall-btn')!;
    this.confirmUninstallBtn = document.getElementById('confirm-uninstall-btn')!;
    this.cleanDbCheckbox = document.getElementById('clean-db-checkbox') as HTMLInputElement;
    this.osPlatformBadge = document.getElementById('os-platform-badge')!;

    this.initEventListeners();
    this.loadClips();
    this.initTauriListeners();
    this.loadSettings();
    this.initReminderScheduler();

    // Start directly as the ultra-compact micro-pill
    this.setNotchState('pill');
  }

  private collapseWindowTimer: any = null;

  private closeAllModals() {
    if (this.historyModal) this.historyModal.classList.add('hidden');
    if (this.reminderModal) this.reminderModal.classList.add('hidden');
    if (this.remindersListModal) this.remindersListModal.classList.add('hidden');
    if (this.deleteModal) this.deleteModal.classList.add('hidden');
    if (this.imageLightbox) this.imageLightbox.classList.add('hidden');
    if (this.uninstallModal) this.uninstallModal.classList.add('hidden');
  }

  private isAnyModalOpen(): boolean {
    return (
      (!!this.historyModal && !this.historyModal.classList.contains('hidden')) ||
      (!!this.reminderModal && !this.reminderModal.classList.contains('hidden')) ||
      (!!this.remindersListModal && !this.remindersListModal.classList.contains('hidden')) ||
      (!!this.deleteModal && !this.deleteModal.classList.contains('hidden')) ||
      (!!this.imageLightbox && !this.imageLightbox.classList.contains('hidden')) ||
      (!!this.uninstallModal && !this.uninstallModal.classList.contains('hidden'))
    );
  }

  private async setNotchState(state: 'pill' | 'preview' | 'expanded') {
    this.currentState = state;
    clearTimeout(this.previewTimer);
    clearTimeout(this.collapseWindowTimer);
    clearTimeout(this.hoverCollapseTimer);

    switch (state) {
      case 'pill':
        this.isExpanded = false;
        this.closeSettingsPanel();
        this.closeAllModals();
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
    this.notchHeader.addEventListener('mousedown', (e) => {
      // Don't trigger window drag if clicking action buttons or filter elements
      if ((e.target as HTMLElement).closest('button, input, a, .icon-btn, .filter-btn')) return;
      if (e.button === 0) {
        this.isDraggingWindow = false;
        this.dragStartTime = Date.now();
        this.dragStartX = e.screenX;
        this.dragStartY = e.screenY;

        const handleMouseMove = async (moveEvt: MouseEvent) => {
          if (!this.isDraggingWindow && moveEvt.buttons === 1) {
            const moveDist = Math.hypot(moveEvt.screenX - this.dragStartX, moveEvt.screenY - this.dragStartY);
            if (moveDist > 5) {
              this.isDraggingWindow = true;
              try {
                await invoke('start_dragging');
              } catch (_) {
                try {
                  await getCurrentWindow().startDragging();
                } catch (_) {}
              }
            }
          }
        };

        const handleMouseUp = () => {
          window.removeEventListener('mousemove', handleMouseMove);
          window.removeEventListener('mouseup', handleMouseUp);
        };

        window.addEventListener('mousemove', handleMouseMove);
        window.addEventListener('mouseup', handleMouseUp);
      }
    });

    this.notchHeader.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('button, input, a, .icon-btn, .filter-btn')) return;
      if (this.isDraggingWindow) return;
      const dragDuration = Date.now() - this.dragStartTime;
      const moveDist = Math.hypot(e.screenX - this.dragStartX, e.screenY - this.dragStartY);
      if (dragDuration > 300 || moveDist > 5) {
        return;
      }
      this.stopPillBounce();
      this.toggleExpand();
    });

    this.notchShell.addEventListener('click', (e) => {
      this.stopPillBounce();
      if (this.currentState === 'pill') {
        if ((e.target as HTMLElement).closest('button, input, a, .icon-btn, .filter-btn')) return;
        if (this.isDraggingWindow) return;
        const dragDuration = Date.now() - this.dragStartTime;
        const moveDist = Math.hypot(e.screenX - this.dragStartX, e.screenY - this.dragStartY);
        if (dragDuration > 300 || moveDist > 5) return;
        this.setNotchState('expanded');
      }
    });

    const toggleExpandBtn = document.getElementById('toggle-expand-btn');
    if (toggleExpandBtn) {
      toggleExpandBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleExpand();
      });
    }

    if (this.notchRemindersBtn) {
      this.notchRemindersBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (this.currentState !== 'expanded') {
          this.setNotchState('expanded');
        }
        this.openRemindersListModal();
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
        if (this.currentState === 'expanded' && !this.isAnyModalOpen()) {
          this.setNotchState('pill');
        }
      }, 350);
    });

    // Collapse back to micro-pill when expanded window loses focus (e.g. clicking background anywhere on screen or another app)
    window.addEventListener('blur', () => {
      if (this.currentState === 'expanded' && !this.isAnyModalOpen()) {
        this.setNotchState('pill');
      }
    });

    // Collapse back to micro-pill when clicking on background outside notch shell
    document.addEventListener('click', (e: MouseEvent) => {
      if (!this.notchShell.contains(e.target as Node) && this.currentState === 'expanded' && !this.isAnyModalOpen()) {
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
    if (this.lightboxInfoBtn) {
      this.lightboxInfoBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (this.lightboxInfoCard) {
          const isHidden = this.lightboxInfoCard.classList.toggle('hidden');
          if (this.lightboxInfoBtn) {
            if (!isHidden) this.lightboxInfoBtn.classList.add('active');
            else this.lightboxInfoBtn.classList.remove('active');
          }
        }
      });
    }

    if (this.closeLightboxInfoBtn) {
      this.closeLightboxInfoBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (this.lightboxInfoCard) this.lightboxInfoCard.classList.add('hidden');
        if (this.lightboxInfoBtn) this.lightboxInfoBtn.classList.remove('active');
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
        this.saveHotkey('Disabled');
      });
    }
    if (this.disableHotkeyBtn) {
      this.disableHotkeyBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.saveHotkey('Disabled');
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

    if (this.uninstallBtn) {
      this.uninstallBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (this.uninstallModal) {
          this.uninstallModal.classList.remove('hidden');
        }
      });
    }
    if (this.closeUninstallModalBtn) {
      this.closeUninstallModalBtn.addEventListener('click', () => {
        if (this.uninstallModal) this.uninstallModal.classList.add('hidden');
      });
    }
    if (this.cancelUninstallBtn) {
      this.cancelUninstallBtn.addEventListener('click', () => {
        if (this.uninstallModal) this.uninstallModal.classList.add('hidden');
      });
    }
    if (this.confirmUninstallBtn) {
      this.confirmUninstallBtn.addEventListener('click', async () => {
        const cleanData = this.cleanDbCheckbox ? this.cleanDbCheckbox.checked : false;
        try {
          await invoke('disable_and_uninstall_app', { cleanData });
        } catch (err) {
          console.error('Failed to disable/uninstall app:', err);
        }
      });
    }
    if (this.uninstallModal) {
      this.uninstallModal.addEventListener('click', (e) => {
        if (e.target === this.uninstallModal) {
          this.uninstallModal.classList.add('hidden');
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
        if (this.isAnyModalOpen()) {
          this.closeAllModals();
        } else if (!this.settingsPanel.classList.contains('hidden')) {
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
      const isMac = /Mac|iPod|iPhone|iPad|Darwin/.test(navigator.userAgent) ||
                    /Mac/.test(navigator.platform) ||
                    (navigator as any).userAgentData?.platform === 'macOS';
      if (this.osPlatformBadge) {
        if (isMac) {
          this.osPlatformBadge.innerHTML = `<svg width="12" height="12" viewBox="0 0 24 24" fill="#ffffff" style="margin-right:5px; vertical-align:-1px; display:inline-block;"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M15.97 6.32c.67-.82 1.13-1.96.99-3.11-1 .04-2.19.67-2.88 1.48-.62.72-1.15 1.88-.99 3.01 1.12.09 2.22-.56 2.88-1.38z"/></svg><span style="color:#ffffff; font-weight:600;">macOS</span>`;
        } else {
          this.osPlatformBadge.innerHTML = `<svg width="11" height="11" viewBox="0 0 88 88" style="margin-right:5px; vertical-align:-1px; display:inline-block;"><path d="M0 12.402l35.687-4.86.016 34.423-35.67.202zm35.67 33.329l.029 34.502L0 75.312V45.928zm4.357-38.932L88 0v41.442l-47.973.473zm47.973 39.467V88L40.027 81.258l-.017-34.786z" fill="#0078D4"/></svg><span style="color:#0078D4; font-weight:600;">Windows</span>`;
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

    if (!shortcutStr || shortcutStr.trim() === '' || shortcutStr.toLowerCase() === 'disabled' || shortcutStr.toLowerCase() === 'none') {
      this.keycapDisplay.innerHTML = `<span style="color: #ef4444; font-weight: 500;">Disabled</span>`;
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
    const targetShortcut = shortcut.trim() || 'Disabled';
    try {
      this.currentShortcut = targetShortcut;
      this.isRecordingHotkey = false;
      this.renderKeycaps(targetShortcut);

      await invoke('save_global_shortcut', { shortcut: targetShortcut });
      if (this.shortcutBadge) {
        this.shortcutBadge.textContent = targetShortcut.toLowerCase() === 'disabled' ? 'Disabled' : targetShortcut;
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
    this.updateHeaderReminderBadge();
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
    const hasActiveReminder = clip.reminder_at && clip.reminder_at * 1000 > Date.now();

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
            ${clip.reminder_at ? `<span class="meta-dot">•</span><span style="color:#f59e0b;">⏰ ${new Date(clip.reminder_at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>` : ''}
          </div>
        </div>
        <div class="clip-right-actions">
          ${isSelected ? '<span class="enter-badge" title="Press Enter to Copy">↵</span>' : ''}
          <div class="action-btn-group">
            <button class="action-btn reminder-btn ${hasActiveReminder ? 'active-reminder' : ''}" title="${clip.reminder_at ? 'Reminder set for ' + new Date(clip.reminder_at * 1000).toLocaleString() : 'Set Reminder'}">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path><path d="M13.73 21a2 2 0 0 1-3.46 0"></path></svg>
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

      const reminderBtn = card.querySelector('.reminder-btn');
      const copyBtn = card.querySelector('.copy-btn');
      const deleteBtn = card.querySelector('.delete-btn');
      const revealBtn = card.querySelector('.reveal-btn');

      if (reminderBtn) {
        reminderBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.stopImmediatePropagation();
          this.openReminderModal(id);
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
        this.stopPillBounce();
        if (clip && clip.category === 'image') {
          this.openLightbox(clip);
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

    // Reminder Modal Events
    if (this.closeReminderModalBtn) {
      this.closeReminderModalBtn.onclick = () => {
        this.reminderModal.classList.add('hidden');
      };
    }
    if (this.cancelReminderBtn) {
      this.cancelReminderBtn.onclick = () => {
        this.reminderModal.classList.add('hidden');
      };
    }
    if (this.saveReminderBtn) {
      this.saveReminderBtn.onclick = () => {
        this.saveReminder();
      };
    }
    if (this.clearReminderBtn) {
      this.clearReminderBtn.onclick = () => {
        this.clearReminder();
      };
    }
    if (this.reminderModal) {
      this.reminderModal.onclick = (e) => {
        if (e.target === this.reminderModal) {
          this.reminderModal.classList.add('hidden');
        }
      };
    }

    const presetButtons = document.querySelectorAll('.preset-btn');
    presetButtons.forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const preset = (btn as HTMLElement).getAttribute('data-preset');
        if (preset) this.setPresetReminderTime(preset);
      });
    });

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

  private openReminderModal(id: string) {
    const clip = this.clips.find((c) => c.id === id);
    if (!clip) return;

    this.activeReminderClipId = id;
    if (clip.category === 'image') {
      this.reminderClipPreview.innerHTML = `<img src="${clip.content}" style="max-height:55px; border-radius:6px;" alt="Clip Image"/>`;
    } else if (clip.is_sensitive) {
      this.reminderClipPreview.textContent = '🔒 Password Protected Clip';
    } else {
      this.reminderClipPreview.textContent = clip.content.length > 85 ? clip.content.slice(0, 85) + '...' : clip.content;
    }

    const targetDate = clip.reminder_at ? new Date(clip.reminder_at * 1000) : new Date(Date.now() + 60 * 60 * 1000);
    this.reminderDatetimeInput.value = this.formatDateForInput(targetDate);

    if (clip.reminder_at) {
      this.clearReminderBtn.classList.remove('hidden');
    } else {
      this.clearReminderBtn.classList.add('hidden');
    }

    this.reminderModal.classList.remove('hidden');
  }

  private formatDateForInput(date: Date): string {
    const pad = (num: number) => String(num).padStart(2, '0');
    const year = date.getFullYear();
    const month = pad(date.getMonth() + 1);
    const day = pad(date.getDate());
    const hours = pad(date.getHours());
    const minutes = pad(date.getMinutes());
    return `${year}-${month}-${day}T${hours}:${minutes}`;
  }

  private setPresetReminderTime(preset: string) {
    const now = new Date();
    if (preset === '15m') {
      now.setMinutes(now.getMinutes() + 15);
    } else if (preset === '1h') {
      now.setHours(now.getHours() + 1);
    } else if (preset === '4h') {
      now.setHours(now.getHours() + 4);
    } else if (preset === 'tomorrow') {
      now.setDate(now.getDate() + 1);
      now.setHours(9, 0, 0, 0);
    }
    this.reminderDatetimeInput.value = this.formatDateForInput(now);
  }

  private async saveReminder() {
    if (!this.activeReminderClipId) return;

    let inputVal = this.reminderDatetimeInput.value;
    if (!inputVal) {
      const fallbackDate = new Date(Date.now() + 60 * 60 * 1000);
      inputVal = this.formatDateForInput(fallbackDate);
      this.reminderDatetimeInput.value = inputVal;
    }

    const selectedDate = new Date(inputVal);
    if (isNaN(selectedDate.getTime())) return;

    const timestampSecs = Math.floor(selectedDate.getTime() / 1000);

    const clip = this.clips.find((c) => c.id === this.activeReminderClipId);
    if (clip) {
      clip.reminder_at = timestampSecs;
      this.triggeredReminderIds.delete(clip.id);
    }

    this.renderClips();
    this.updateHeaderReminderBadge();
    this.reminderModal.classList.add('hidden');
    this.streamText.innerHTML = `<span class="green-check">✓</span> Reminder set for ${selectedDate.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;

    try {
      await invoke('set_clip_reminder', {
        id: this.activeReminderClipId,
        reminder_at: timestampSecs,
        reminderAt: timestampSecs,
      });
    } catch (err) {
      console.warn('Backend set_clip_reminder note:', err);
    }
  }

  private async clearReminder() {
    if (!this.activeReminderClipId) return;
    
    const clip = this.clips.find((c) => c.id === this.activeReminderClipId);
    if (clip) {
      clip.reminder_at = null;
      this.triggeredReminderIds.delete(clip.id);
    }

    this.renderClips();
    this.updateHeaderReminderBadge();
    this.reminderModal.classList.add('hidden');
    this.streamText.textContent = `Reminder cleared.`;

    try {
      await invoke('set_clip_reminder', {
        id: this.activeReminderClipId,
        reminder_at: null,
        reminderAt: null,
      });
    } catch (err) {
      console.warn('Backend clear_reminder note:', err);
    }
  }

  private initReminderScheduler() {
    this.reminderCheckInterval = setInterval(() => {
      const nowSecs = Math.floor(Date.now() / 1000);
      for (const clip of this.clips) {
        if (clip.reminder_at && Number(clip.reminder_at) <= nowSecs) {
          if (!this.triggeredReminderIds.has(clip.id)) {
            this.triggeredReminderIds.add(clip.id);
            this.triggerReminderBounce(clip);
          }
        }
      }
    }, 1000);
  }

  private stopPillBounce() {
    this.streamText.classList.remove('jump-8s', 'jump-5s');
    this.notchShell.classList.remove('reminder-bounce');
  }

  private async triggerReminderBounce(clip: ClipItem) {
    try {
      await invoke('show_window');
    } catch (_) {}

    await this.setNotchState('preview');

    let textPreview = clip.content.replace(/\r?\n/g, ' ').trim();
    if (clip.is_sensitive) textPreview = '🔒 Password Protected Clip';
    else if (clip.category === 'image') textPreview = '🖼️ Screenshot';
    else if (textPreview.length > 35) textPreview = textPreview.slice(0, 35) + '...';

    this.streamText.innerHTML = `⏰ <b>Reminder:</b> "${this.escapeHTML(textPreview || 'Clip')}"`;

    this.streamText.classList.add('jump-8s');
    setTimeout(() => {
      this.streamText.classList.remove('jump-8s');
    }, 8000);

    clearTimeout(this.previewTimer);
    this.previewTimer = setTimeout(() => {
      if (this.currentState === 'preview' && !this.isAnyModalOpen()) {
        this.setNotchState('pill');
      }
    }, 9500);
  }

  private updateHeaderReminderBadge() {
    const activeCount = this.clips.filter((c) => c.reminder_at && c.reminder_at * 1000 > Date.now()).length;
    if (activeCount > 0) {
      if (this.notchRemindersBtn) this.notchRemindersBtn.classList.add('has-reminders');
    } else {
      if (this.notchRemindersBtn) this.notchRemindersBtn.classList.remove('has-reminders');
    }
  }

  private openRemindersListModal() {
    this.renderRemindersList();
    if (this.remindersListModal) {
      this.remindersListModal.classList.remove('hidden');
    }
  }

  private renderRemindersList() {
    if (!this.remindersManagerList) return;

    const activeClips = this.clips.filter((c) => c.reminder_at && c.reminder_at > 0);

    if (activeClips.length === 0) {
      this.remindersManagerList.innerHTML = `
        <div class="empty-state" style="padding: 28px 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;">
          <div class="empty-icon" style="font-size: 28px;">⏰</div>
          <p style="font-size: 13px; font-weight: 600; color: var(--text-main); margin: 0;">No active reminders</p>
          <span style="font-size: 11px; color: var(--text-dim);">Click the alarm icon on any clip card to set a reminder.</span>
        </div>
      `;
    } else {
      this.remindersManagerList.innerHTML = activeClips
        .map((clip) => {
          const date = new Date(clip.reminder_at! * 1000);
          const timeStr = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
          const dateStr = date.toLocaleDateString([], { month: 'short', day: 'numeric' });
          
          let previewText = clip.content.replace(/\r?\n/g, ' ').trim();
          if (clip.is_sensitive) previewText = '🔒 Password Protected Clip';
          else if (clip.category === 'image') previewText = '🖼️ Screenshot';
          else if (previewText.length > 45) previewText = previewText.slice(0, 45) + '...';

          return `
            <div class="reminder-manager-card" data-clip-id="${clip.id}">
              <div class="reminder-manager-info">
                <span class="reminder-manager-text">${this.escapeHTML(previewText || '(empty clip)')}</span>
                <span class="reminder-manager-time">⏰ ${dateStr} at ${timeStr}</span>
              </div>
              <div class="reminder-manager-actions">
                <button class="btn-edit-reminder" title="Edit Date/Time" data-id="${clip.id}">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                </button>
                <button class="btn-delete-reminder" title="Delete Reminder" data-id="${clip.id}">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                </button>
              </div>
            </div>
          `;
        })
        .join('');
    }

    const editBtns = this.remindersManagerList.querySelectorAll('.btn-edit-reminder');
    editBtns.forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const id = (btn as HTMLElement).getAttribute('data-id');
        if (id) {
          if (this.remindersListModal) this.remindersListModal.classList.add('hidden');
          this.openReminderModal(id);
        }
      });
    });

    const deleteBtns = this.remindersManagerList.querySelectorAll('.btn-delete-reminder');
    deleteBtns.forEach((btn) => {
      btn.addEventListener('click', async (e) => {
        e.stopPropagation();
        const id = (btn as HTMLElement).getAttribute('data-id');
        if (id) {
          this.activeReminderClipId = id;
          await this.clearReminder();
          this.renderRemindersList();
          this.updateHeaderReminderBadge();
        }
      });
    });

    if (this.closeRemindersListModalBtn) {
      this.closeRemindersListModalBtn.onclick = () => {
        if (this.remindersListModal) this.remindersListModal.classList.add('hidden');
      };
    }

    if (this.remindersListModal) {
      this.remindersListModal.onclick = (e) => {
        if (e.target === this.remindersListModal) {
          this.remindersListModal.classList.add('hidden');
        }
      };
    }
  }

  private openLightbox(clipOrSrc: ClipItem | string) {
    let clip: ClipItem | undefined;
    let contentSrc = '';

    if (typeof clipOrSrc === 'string') {
      contentSrc = clipOrSrc;
      clip = this.clips.find((c) => c.content === clipOrSrc);
    } else {
      clip = clipOrSrc;
      contentSrc = clip.content;
    }

    const src = contentSrc.startsWith('data:image/') ? contentSrc : `data:image/png;base64,${contentSrc}`;
    this.lightboxImg.src = src;
    this.activeLightboxClip = clip || null;

    if (this.lightboxInfoCard) {
      this.lightboxInfoCard.classList.add('hidden');
    }
    if (this.lightboxInfoBtn) {
      this.lightboxInfoBtn.classList.remove('active');
    }

    if (clip) {
      const appName = clip.source_app && clip.source_app !== 'Unknown App' ? clip.source_app.replace(/\.exe$/i, '') : 'Clipboard';
      const dateStr = new Date(clip.created_at * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
      if (this.lbInfoApp) this.lbInfoApp.textContent = appName;
      if (this.lbInfoDate) this.lbInfoDate.textContent = dateStr;
      if (this.lbInfoPastes) this.lbInfoPastes.textContent = `${clip.paste_count || 1} time${(clip.paste_count || 1) === 1 ? '' : 's'}`;

      this.lightboxImg.onload = () => {
        if (this.lbInfoDim) {
          this.lbInfoDim.textContent = `${this.lightboxImg.naturalWidth} × ${this.lightboxImg.naturalHeight} px`;
        }
      };
    }

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
