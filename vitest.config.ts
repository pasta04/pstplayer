import { svelte, vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

// Web セッション側でフロントのロジック / reactivity を検証するための
// vitest 設定。jsdom 上で Svelte コンポーネントをマウントできる。
// ネイティブ (libmpv / 窓 / 複数プロセス) は実機 QA でしか確認できないが、
// レス数などの reactivity・描画ロジックはここで自動検証する。
export default defineConfig({
	// configFile: false — svelte.config.js (style 込みの vitePreprocess) を
	// 読まない。テストでは CSS preprocess が vite6/vitest2 で proxy エラーに
	// なるため、script(TS) のみ preprocess する。runes は使用箇所から自動検出。
	plugins: [svelte({ configFile: false, preprocess: vitePreprocess({ style: false }) })],
	resolve: {
		alias: {
			$lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
			$app: fileURLToPath(new URL('./src/test/app-stubs', import.meta.url)),
		},
		conditions: ['browser'],
	},
	test: {
		environment: 'jsdom',
		globals: true,
		include: ['src/**/*.{test,spec}.{js,ts}'],
		setupFiles: ['./src/test/setup.ts'],
	},
});
