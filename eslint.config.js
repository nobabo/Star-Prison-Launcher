import js from '@eslint/js'
import globals from 'globals'

const commonGlobals = {
    ...globals.es2024,
    ...globals.node
}

export default [
    {
        ignores: [
            '.files/**',
            '.tmp/**',
            '.workers/**',
            'node_modules/**',
            'src-tauri/target/**',
            'src-tauri/gen/**',
            'src/vendor/**',
            'target/**'
        ]
    },
    {
        linterOptions: {
            reportUnusedDisableDirectives: 'warn'
        }
    },
    js.configs.recommended,
    {
        files: ['**/*.js', '**/*.mjs'],
        languageOptions: {
            ecmaVersion: 'latest',
            sourceType: 'module',
            globals: commonGlobals
        },
        rules: {
            'no-unused-vars': ['warn', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }]
        }
    },
    {
        files: ['src/**/*.js'],
        languageOptions: {
            globals: {
                ...commonGlobals,
                ...globals.browser
            }
        }
    }
]
