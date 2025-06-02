find . -type f -name "*.rs" \( -path "*/src/*" -o -path "*/tests/*" \) | while IFS= read -r file; do
    if ! grep -q "SPDX-License-Identifier" "$file"; then
        tmpfile=$(mktemp)
        sed 's/^/\/\/ /' .midnight.txt >> "$tmpfile"
        echo "" >> $tmpfile
        cat $file >> $tmpfile
        mv $tmpfile $file
    fi
done

find . -type f -name "*.js" \( -path "*/src/*" -o -path "*/tests/*" \) | while IFS= read -r file; do
    if ! grep -q "SPDX-License-Identifier" "$file"; then
        tmpfile=$(mktemp)
        sed 's/^/\/\/ /' .midnight.txt >> "$tmpfile"
        echo "" >> $tmpfile
        cat $file >> $tmpfile
        mv $tmpfile $file
    fi
done

find . -type f -name "*.sh" | while IFS= read -r file; do
    if ! grep -q "SPDX-License-Identifier" "$file"; then
        tmpfile=$(mktemp)
        echo "#!/usr/bin/env bash" >> $tmpfile
        echo "" >> $tmpfile
        sed 's/^/# /' .midnight.txt >> "$tmpfile"
        echo "" >> $tmpfile
        cat $file >> $tmpfile
        mv $tmpfile $file
    fi
done
