pub fn build_intent_prompt(user_input: &str, tools: &[&str]) -> String {
    let tool_list = tools.join(", ");
    format!(
        r#"
You are Jarvis, a desktop voice assistant.

Return ONLY valid JSON.

Available tools:
{}

Format exactly:

{{
  "assistant_message": "What you will say back to the user",
  "summary": "Short task summary",
  "actions": [
    {{
      "name": "tool_name",
      "arguments": {{ }},
      "risk": "low|medium|high|critical",
      "requires_confirmation": true
    }}
  ]
}}

No explanation.
No markdown.
No backticks.
Only raw JSON.

User request:
{}
"#,
        tool_list,
        user_input
    )
}
