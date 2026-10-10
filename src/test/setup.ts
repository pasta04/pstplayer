// jsdom に無い API を最小限ポリフィルする (Svelte コンポーネントのマウント用)。
import { vi } from 'vitest';

class ResizeObserverStub {
	observe() {}
	unobserve() {}
	disconnect() {}
}
(globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;

if (!window.matchMedia) {
	window.matchMedia = vi.fn().mockImplementation((query: string) => ({
		matches: false,
		media: query,
		onchange: null,
		addEventListener: () => {},
		removeEventListener: () => {},
		addListener: () => {},
		removeListener: () => {},
		dispatchEvent: () => false,
	})) as unknown as typeof window.matchMedia;
}
