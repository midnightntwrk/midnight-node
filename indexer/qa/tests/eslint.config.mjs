import eslint from '@eslint/js';
import tseslint from '@typescript-eslint/eslint-plugin';
import tsparser from '@typescript-eslint/parser';
import prettier from 'eslint-config-prettier';

export default [
  eslint.configs.recommended,
  {
    files: ['**/*.ts', '**/*.tsx', '**/*.js', '**/*.jsx'],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
      },
      globals: {
        // Node.js globals
        console: 'readonly',
        process: 'readonly',
        Buffer: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        setTimeout: 'readonly',
        setInterval: 'readonly',
        clearTimeout: 'readonly',
        clearInterval: 'readonly',
        // Vitest globals (from vitest/globals)
        describe: 'readonly',
        test: 'readonly',
        it: 'readonly',
        expect: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        vi: 'readonly',
        suite: 'readonly',
        context: 'readonly',
        assert: 'readonly',
        // Browser/Web APIs that Node.js also provides
        fetch: 'readonly',
        WebSocket: 'readonly',
        MessageEvent: 'readonly',
        AbortSignal: 'readonly',
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
    },
    rules: {
      ...tseslint.configs.recommended.rules,
      // Allow unused vars if they start with underscore
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
          // Allow unused variables in imports that might be used by types
          varsIgnorePattern: '^_',
        },
      ],
      // Allow explicit any when needed
      '@typescript-eslint/no-explicit-any': 'warn',
      // Allow hasOwnProperty usage
      'no-prototype-builtins': 'off',
      // Allow unsafe optional chaining in test code
      'no-unsafe-optional-chaining': 'warn',
      // Allow redeclare in test code
      'no-redeclare': 'off',
      // Allow case declarations
      'no-case-declarations': 'off',
      // Allow empty blocks in test placeholders
      'no-empty': 'warn',
      // Allow non-null asserted optional chains in tests
      '@typescript-eslint/no-non-null-asserted-optional-chain': 'warn',
      // Allow empty interfaces
      '@typescript-eslint/no-empty-object-type': 'warn',
      // Disable rules that conflict with Prettier
      ...prettier.rules,
    },
  },
  {
    ignores: [
      'node_modules/**',
      'logs/**',
      'reports/**',
      'midnight-indexer/**',
      '**/*.config.js',
      '**/*.config.ts',
      '**/*.config.mjs',
      'data/**',
      '.tmp/**',
    ],
  },
];
