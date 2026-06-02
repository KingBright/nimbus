import json

log_path = "/Users/jinliang/.gemini/antigravity/brain/58e18069-87a5-4013-a8d1-866458c2ee36/.system_generated/logs/transcript.jsonl"

with open(log_path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f"Total steps: {len(lines)}")
# Print tool call args for steps between 1400 and 1436
for line in lines[1400:1436]:
    try:
        obj = json.loads(line)
        step = obj.get("step_index")
        stype = obj.get("type")
        source = obj.get("source")
        
        print(f"--- Step {step} | {source} | {stype} ---")
        
        tool_calls = obj.get("tool_calls", [])
        if tool_calls:
            for c in tool_calls:
                print(f"Tool: {c.get('name')}")
                print(json.dumps(c.get('args'), indent=2, ensure_ascii=False))
        
        # If it's a code action, print diff content
        if stype == "CODE_ACTION":
            print("Content:")
            print(obj.get("content"))
            
    except Exception as e:
        print("Error parsing line:", e)
