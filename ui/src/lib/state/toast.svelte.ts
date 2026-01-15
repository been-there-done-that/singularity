/**
 * Toast State - Svelte 5 Runes
 */

interface Toast {
    id: string;
    type: 'success' | 'error' | 'warning' | 'info';
    message: string;
    duration?: number;
}

class ToastStore {
    toasts = $state<Toast[]>([]);

    add(message: string, type: Toast['type'] = 'info', duration = 5000) {
        const id = crypto.randomUUID();
        this.toasts = [...this.toasts, { id, message, type, duration }];

        setTimeout(() => {
            this.remove(id);
        }, duration);
    }

    remove(id: string) {
        this.toasts = this.toasts.filter((t) => t.id !== id);
    }

    success(message: string, duration?: number) { this.add(message, 'success', duration); }
    error(message: string, duration?: number) { this.add(message, 'error', duration); }
    warning(message: string, duration?: number) { this.add(message, 'warning', duration); }
    info(message: string, duration?: number) { this.add(message, 'info', duration); }
}

export const toast = new ToastStore();
