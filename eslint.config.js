import js from '@eslint/js';
import ts from '@typescript-eslint/eslint-plugin';
import tsParser from '@typescript-eslint/parser';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import globals from 'globals';

const sharedGlobals = { ...globals.browser, ...globals.es2022 };

export default [
	js.configs.recommended,
	{
		files: ['**/*.ts'],
		languageOptions: {
			parser: tsParser,
			parserOptions: { ecmaVersion: 2022, sourceType: 'module' },
			globals: sharedGlobals,
		},
		plugins: { '@typescript-eslint': ts },
		rules: {
			...ts.configs.recommended.rules,
			'@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
		},
	},
	{
		files: ['**/*.svelte'],
		languageOptions: {
			parser: svelteParser,
			parserOptions: { parser: tsParser, ecmaVersion: 2022, sourceType: 'module' },
			globals: sharedGlobals,
		},
		plugins: { svelte },
		rules: {
			...(svelte.configs['flat/recommended']?.[1]?.rules ?? {}),
		},
	},
	{
		files: ['vite.config.js', 'svelte.config.js', 'eslint.config.js'],
		languageOptions: {
			globals: { ...globals.node, ...globals.es2022 },
		},
	},
	{
		ignores: [
			'.svelte-kit/',
			'build/',
			'node_modules/',
			'src-tauri/target/',
			'target/',
			'docs/',
			'**/*.json',
			'**/*.md',
		],
	},
];
