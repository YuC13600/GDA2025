#!/usr/bin/env python3
"""
Anime selection script using Claude Haiku API.

This script uses Claude to intelligently select the best matching anime
from ani-cli search results based on MAL metadata.
"""

import os
import sys
import json
import re
import argparse
from typing import Dict, List, Any, Optional, Tuple

try:
    import anthropic
except ImportError:
    print(json.dumps({
        "error": "anthropic package not installed. Run: pip install anthropic",
        "index": 0,
        "confidence": "error"
    }))
    sys.exit(1)


def parse_episode_count(title: str) -> Optional[int]:
    """Extract episode count from anime title string like 'Title (64 eps)'"""
    match = re.search(r'\((\d+)\s+eps?\)', title, re.IGNORECASE)
    if match:
        return int(match.group(1))
    return None


def validate_episode_match(mal_episodes: Optional[int], selected_episodes: Optional[int]) -> Tuple[str, Optional[str]]:
    """
    Validate episode count match between MAL and selected anime.

    Returns:
        (match_status, adjusted_confidence) where:
        - match_status: "exact", "close", "acceptable", "mismatch", or "unknown"
        - adjusted_confidence: None or "low"/"medium" if confidence should be downgraded
    """
    if mal_episodes is None or selected_episodes is None:
        return ("unknown", None)

    diff = abs(selected_episodes - mal_episodes)
    diff_percent = diff / mal_episodes if mal_episodes > 0 else 0

    if diff == 0:
        return ("exact", None)
    elif diff <= 2:
        return ("close", None)  # Acceptable difference (±2 episodes)
    elif diff <= 5 or diff_percent <= 0.1:
        return ("acceptable", None)  # Small difference (<10%)
    else:
        # Large difference - likely selected wrong anime
        return ("mismatch", "low")


def create_selection_prompt(mal_info: Dict[str, Any], candidates: List[str]) -> str:
    """Create the prompt for Claude to select the best anime match."""

    # Format candidates list
    candidates_text = "\n".join(
        f"{i+1}. {candidate}"
        for i, candidate in enumerate(candidates)
    )

    prompt = f"""You are an anime title matching expert. Your task is to select the BEST matching anime from a list of search results.

MAL (MyAnimeList) Information:
- Title: "{mal_info['title']}"
- Episodes: {mal_info.get('episodes', 'Unknown')}
- Year: {mal_info.get('year', 'Unknown')}
- Type: {mal_info.get('anime_type', 'Unknown')}

Available Candidates from ani-cli search:
{candidates_text}

Selection Criteria (in order of importance):
1. **Main series vs Specials/OVA**: Strongly prefer the main TV series over specials, recaps, or OVAs
2. **Episode count**: The candidate should have a similar number of episodes to the MAL data
3. **Series vs Season**: If the anime has multiple seasons, match the correct season
4. **Title similarity**: Consider romanization variants and alternative titles
5. **Year**: Should be close to the MAL year (within 1-2 years is acceptable)

IMPORTANT NOTES:
- "Specials", "Recap", "OVA", "ONA" usually indicate extra content, NOT the main series
- If episode count differs significantly (>3 episodes), it's likely the wrong match
- Be cautious with very short titles that might match multiple series
- If no good match exists, select the closest one but mark confidence as "low"

Respond with ONLY valid JSON (no markdown, no explanation outside JSON):
{{
  "index": <number from 1 to {len(candidates)}>,
  "confidence": "high|medium|low",
  "reason": "<brief 1-sentence explanation of why this match was selected>"
}}"""

    return prompt


def select_anime_with_claude(
    mal_info: Dict[str, Any],
    candidates: List[str],
    api_key: Optional[str] = None
) -> Dict[str, Any]:
    """
    Use Claude Haiku to select the best matching anime.

    Args:
        mal_info: Dictionary containing MAL metadata (title, episodes, year, type)
        candidates: List of anime titles from ani-cli search results
        api_key: Anthropic API key (if None, uses ANTHROPIC_API_KEY env var)

    Returns:
        Dictionary with keys: index (1-based), confidence, reason
    """

    if not candidates:
        return {
            "error": "No candidates provided",
            "index": 0,
            "confidence": "error"
        }

    # If only one candidate, return it directly
    if len(candidates) == 1:
        return {
            "index": 1,
            "confidence": "high",
            "reason": "Only one candidate available"
        }

    # Get API key
    if api_key is None:
        api_key = os.getenv("ANTHROPIC_API_KEY")

    if not api_key:
        return {
            "error": "ANTHROPIC_API_KEY not set in environment",
            "index": 0,
            "confidence": "error"
        }

    try:
        client = anthropic.Anthropic(api_key=api_key)

        prompt = create_selection_prompt(mal_info, candidates)

        message = client.messages.create(
            model="claude-3-5-haiku-20241022",
            max_tokens=300,
            temperature=0.0,  # Deterministic selection
            messages=[{
                "role": "user",
                "content": prompt
            }]
        )

        # Extract JSON from response
        response_text = message.content[0].text.strip()

        # Remove markdown code blocks if present
        if response_text.startswith("```"):
            lines = response_text.split("\n")
            response_text = "\n".join(lines[1:-1])
        if response_text.startswith("json"):
            response_text = response_text[4:].strip()

        result = json.loads(response_text)

        # Validate response
        if "index" not in result or "confidence" not in result:
            return {
                "error": "Invalid response format from Claude",
                "index": 1,  # Fallback to first candidate
                "confidence": "low",
                "reason": "API response was malformed"
            }

        # Validate index is in valid range
        index = result["index"]
        if not isinstance(index, int) or index < 1 or index > len(candidates):
            result["index"] = 1
            result["confidence"] = "low"
            result["reason"] = f"Invalid index {index}, using first candidate"

        # Validate episode count match
        selected_title = candidates[index - 1]
        selected_episodes = parse_episode_count(selected_title)
        mal_episodes = mal_info.get('episodes')

        episode_match, confidence_adjustment = validate_episode_match(mal_episodes, selected_episodes)

        # Adjust confidence if episode mismatch
        if confidence_adjustment:
            if result["confidence"] == "high":
                result["confidence"] = "medium"
            elif result["confidence"] == "medium":
                result["confidence"] = "low"

        # Add episode validation info to result
        result["mal_episodes"] = mal_episodes
        result["selected_episodes"] = selected_episodes
        result["episode_match"] = episode_match

        return result

    except json.JSONDecodeError as e:
        return {
            "error": f"Failed to parse Claude response: {e}",
            "index": 1,
            "confidence": "low",
            "reason": "JSON parsing error"
        }
    except Exception as e:
        return {
            "error": f"API call failed: {e}",
            "index": 1,
            "confidence": "low",
            "reason": f"Exception: {type(e).__name__}"
        }


def main():
    parser = argparse.ArgumentParser(
        description="Select best anime match using Claude Haiku"
    )
    parser.add_argument("--mal-title", required=True, help="Anime title from MAL")
    parser.add_argument("--episodes", type=int, help="Number of episodes from MAL")
    parser.add_argument("--year", type=int, help="Year from MAL")
    parser.add_argument("--anime-type", help="Anime type from MAL (TV, Movie, etc)")
    parser.add_argument("--candidates", required=True, help="JSON array of candidate titles")
    parser.add_argument("--api-key", help="Anthropic API key (optional, uses env var if not provided)")

    args = parser.parse_args()

    # Parse candidates JSON
    try:
        candidates = json.loads(args.candidates)
    except json.JSONDecodeError:
        print(json.dumps({
            "error": "Invalid JSON in candidates argument",
            "index": 0,
            "confidence": "error"
        }))
        sys.exit(1)

    # Build MAL info dictionary
    mal_info = {
        "title": args.mal_title,
        "episodes": args.episodes,
        "year": args.year,
        "anime_type": args.anime_type
    }

    # Call Claude for selection
    result = select_anime_with_claude(mal_info, candidates, args.api_key)

    # Output result as JSON
    print(json.dumps(result, ensure_ascii=False))

    # Exit with error code if selection failed
    if "error" in result:
        sys.exit(1)

    sys.exit(0)


if __name__ == "__main__":
    main()
