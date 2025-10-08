// @ts-check

import globals from "globals";
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  eslint.configs.recommended,
  tseslint.configs.strict,
  tseslint.configs.stylistic,
  {
    ignores: ["dist/", ".papi/**"],
  },
  {
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
);
