#!/bin/bash
# Get anime candidates from AllAnime API (used by ani-cli)
# Returns JSON array of candidate names

QUERY="$1"

if [ -z "$QUERY" ]; then
    echo '{"error": "No query provided"}' >&2
    exit 1
fi

# AllAnime API settings (from ani-cli source)
allanime_base="allanime.day"
allanime_api="https://api.$allanime_base"
allanime_refr="https://allanime.to"  # IMPORTANT: Must use .to (main site) as referer!
agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/121.0"
mode="sub"  # sub or dub

# GraphQL query (from ani-cli)
search_gql='query(        $search: SearchInput        $limit: Int        $page: Int        $translationType: VaildTranslationTypeEnumType        $countryOrigin: VaildCountryOriginEnumType    ) {    shows(        search: $search        limit: $limit        page: $page        translationType: $translationType        countryOrigin: $countryOrigin    ) {        edges {            _id name availableEpisodes __typename       }    }}'

# Call API
result=$(curl -e "$allanime_refr" -s -G "${allanime_api}/api" \
    --data-urlencode "variables={\"search\":{\"allowAdult\":true,\"allowUnknown\":false,\"query\":\"$QUERY\"},\"limit\":10,\"page\":1,\"translationType\":\"$mode\",\"countryOrigin\":\"ALL\"}" \
    --data-urlencode "query=$search_gql" \
    -A "$agent" 2>/dev/null)

if [ $? -ne 0 ] || [ -z "$result" ]; then
    echo '{"error": "API request failed"}' >&2
    exit 1
fi

# Parse JSON and extract names with episode counts
# Output format: ["Name1 (12 eps)", "Name2 (6 eps)", ...]
candidates=$(echo "$result" | sed 's|Show|\n|g' | \
    sed -nE "s|.*\"name\":\"([^\"]*)\".*\"${mode}\":([1-9][^,]*).*|\1 (\2 eps)|p" | \
    python3 -c 'import sys, json; print(json.dumps([line.strip() for line in sys.stdin if line.strip()]))')

if [ -z "$candidates" ] || [ "$candidates" = "[]" ] || [ "$candidates" = "null" ]; then
    echo '{"error": "No candidates found"}' >&2
    exit 1
fi

echo "$candidates"
exit 0
