// SvelteKit の $app/navigation をテスト環境向けにスタブする。
export const goto = () => Promise.resolve();
export const invalidate = () => Promise.resolve();
export const invalidateAll = () => Promise.resolve();
