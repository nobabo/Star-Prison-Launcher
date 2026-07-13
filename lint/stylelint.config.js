export default {
    extends: ['stylelint-config-standard'],
    ignoreFiles: ['.files/**', '.tmp/**', 'node_modules/**', 'src-tauri/target/**', 'src/vendor/**', 'target/**'],
    rules: {
        'alpha-value-notation': null,
        'color-function-alias-notation': null,
        'color-function-notation': null,
        'color-hex-length': null,
        'custom-property-empty-line-before': null,
        'declaration-block-no-redundant-longhand-properties': null,
        'declaration-empty-line-before': null,
        'keyframes-name-pattern': null,
        'length-zero-no-unit': null,
        'media-feature-range-notation': null,
        'no-descending-specificity': null,
        'property-no-vendor-prefix': null,
        'selector-class-pattern': null
    }
}
