#!/bin/bash
# 修復 anime-selector 失敗的 8 部動畫

echo "🔧 開始修復失敗的 8 部動畫..."
echo ""

# 失敗的 mal_id 列表
FAILED_IDS=(1564 1770 2564 3470 10161 32316 33051 48441)

success_count=0
failed_count=0
failed_list=()

for mal_id in "${FAILED_IDS[@]}"; do
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "📌 處理 mal_id: $mal_id"
    
    # 獲取動畫標題
    title=$(sqlite3 data/jobs.db "SELECT title FROM anime WHERE mal_id = $mal_id;")
    echo "   標題: $title"
    echo ""
    
    # 執行 anime-selector
    if cargo run --release -p anime-selector -- --mal-id $mal_id 2>&1 | tee /tmp/selector_$mal_id.log | grep -q "Selection complete"; then
        echo "✅ 成功: $mal_id - $title"
        ((success_count++))
    else
        echo "❌ 失敗: $mal_id - $title"
        ((failed_count++))
        failed_list+=("$mal_id - $title")
    fi
    echo ""
done

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 修復完成總結"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ 成功: $success_count"
echo "❌ 失敗: $failed_count"

if [ $failed_count -gt 0 ]; then
    echo ""
    echo "仍然失敗的動畫："
    for item in "${failed_list[@]}"; do
        echo "  - $item"
    done
fi

echo ""
echo "📋 檢查結果："
sqlite3 data/jobs.db "
SELECT 
  'Total anime needing selection' as status, COUNT(DISTINCT j.mal_id) as count
FROM jobs j 
LEFT JOIN anime_selection_cache s ON j.mal_id = s.mal_id 
WHERE s.mal_id IS NULL
UNION ALL
SELECT 'Total jobs needing selection', COUNT(*)
FROM jobs j 
LEFT JOIN anime_selection_cache s ON j.mal_id = s.mal_id 
WHERE s.mal_id IS NULL;
"
