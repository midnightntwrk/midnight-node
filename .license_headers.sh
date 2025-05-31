find . -type f -name "*.rs" \( -path "*/src/*" -o -path "*/tests/*" \) | while IFS= read -r file; do
    if ! grep -q "SPDX-License-Identifier" "$file"; then
        tmpfile=$(mktemp)
        cat .midnight.txt >> $tmpfile
        echo "" >> $tmpfile
        cat $file >> $tmpfile
        mv $tmpfile $file
    fi
done
