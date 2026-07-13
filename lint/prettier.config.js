export default {
    printWidth: 120,
    semi: false,
    singleQuote: true,
    tabWidth: 4,
    trailingComma: 'none',
    bracketSameLine: false,
    overrides: [
        {
            files: ['*.json', '*.yml', '*.yaml'],
            options: {
                tabWidth: 2
            }
        },
        {
            files: ['*.md'],
            options: {
                proseWrap: 'preserve'
            }
        }
    ]
}
