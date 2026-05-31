import re
import sys

def process(content):
    # Change the signature
    content = content.replace("pub fn process_input(document: &mut Document, input_text: &mut String) -> Option<String>", 
                              "pub struct CommandResult { pub message: String, pub switch_to_3d: bool }\n\npub fn process_input(document: &mut Document, input_text: &mut String) -> Option<CommandResult>")

    # Replace Option<String> with Option<CommandResult> in result variable declaration
    content = content.replace("let mut result: Option<String> = None;", "let mut result: Option<CommandResult> = None;")

    # We need to replace `Some(expr)` with `Some(CommandResult { message: expr, switch_to_3d: false })`
    # But wait, there are also `return Some(expr)`.
    # Let's use a regex to match `Some(...)` where it's not already CommandResult.
    
    # Actually, simpler: replace `.into()` with `.into()` since most strings are returned with `.into()` or `format!(...)`.
    # Let's manually replace `Some(` with a custom replacement function
    def replacer(match):
        inner = match.group(1)
        if "CommandResult" in inner:
            return match.group(0)
        
        # Check if this is one of the 3D attractor or surface commands
        is_3d = False
        if any(kw in inner.lower() for kw in ["attractor created", "surface3d", "sphere created", "cube created", "pyramid", "cone", "cylinder", "parametriccurve3d", "vectorfield3d", "hypercube", "hypersphere"]):
            is_3d = True
            
        return f"Some(CommandResult {{ message: {inner}, switch_to_3d: {'true' if is_3d else 'false'} }})"
    
    content = re.sub(r'Some\((.*?\.into\(\)|format!.*?)\)', replacer, content)
    
    # Some other specific replacements for result assignments
    content = content.replace("Some(if res.is_empty() { \"No extrema found in [-10,10]\".into() } else { res })", 
                              "Some(CommandResult { message: if res.is_empty() { \"No extrema found in [-10,10]\".into() } else { res }, switch_to_3d: false })")
    
    content = content.replace("Some(output)", "Some(CommandResult { message: output, switch_to_3d: false })")
    content = content.replace("Some(poly.vertices.clone())", "Some(poly.vertices.clone())") # wait, this is inside a different function?
    
    return content

with open("grafito-app/src/commands.rs", "r") as f:
    text = f.read()

text = process(text)

with open("grafito-app/src/commands.rs", "w") as f:
    f.write(text)

print("Refactored commands.rs")
