#!/usr/bin/env python3
import re
import sys
import argparse
from pathlib import Path

def update_latex_macros(file_path, macro_name, value, format_type):
    """
    Update LaTeX macro definitions in a file.
    
    Args:
        file_path: Path to the file containing LaTeX macros
        macro_name: Name of the macro to update/add (without the backslash)
        value: The value to set for the macro
        format_type: Format type for the macro ('date', 'percent', 'number')
    
    Returns:
        True if file was updated, False otherwise
    """
    # Read the existing file
    try:
        with open(file_path, 'r') as f:
            lines = f.readlines()
    except FileNotFoundError:
        lines = []
    
    # Prepare the new command based on format type
    if format_type == 'date':
        new_command = f"\\newcommand{{\\{macro_name}}}{{{value}\\xspace}}\n"
    elif format_type == 'percent':
        new_command = f"\\newcommand{{\\{macro_name}}}{{\\num{{{value}}}\\%\\xspace}}\n"
    elif format_type == 'number':
        if ' ' in str(value).lower():  # Check if value has a unit part like "0.5 million"
            value_parts = str(value).split(' ', 1)
            new_command = f"\\newcommand{{\\{macro_name}}}{{\\num{{{value_parts[0].replace(',', ' ')}}} {value_parts[1]}\\xspace}}\n"
        else:
            new_command = f"\\newcommand{{\\{macro_name}}}{{\\num{{{value.replace(',', ' ')}}}\\xspace}}\n"
    # elif format_type == 'custom':
    #     new_command = f"\\newcommand{{\\{macro_name}}}{{{value}}}\n"
    else:
        print(f"Unknown format type: {format_type}")
        return False

    # Check if the macro already exists
    pattern = re.compile(f"\\\\newcommand{{\\\\{macro_name}}}{{.*}}")
    macro_exists = False
    
    for i, line in enumerate(lines):
        if pattern.match(line):
            lines[i] = new_command
            macro_exists = True
            break
    
    # If macro doesn't exist, add it to the end
    if not macro_exists:
        lines.append(new_command)
    
    # Write the updated content back to the file
    with open(file_path, 'w') as f:
        f.writelines(lines)
    
    return True

def main():
    parser = argparse.ArgumentParser(description='Update LaTeX macro definitions in a file')
    parser.add_argument('file_path', help='Path to the file containing the LaTeX macros')
    parser.add_argument('macro_name', help='Name of the macro to update/add (without the backslash)')
    parser.add_argument('value', help='Value to set for the macro')
    parser.add_argument('--format', choices=['date', 'percent', 'number', 'custom'], 
                        default='custom', help='Format type for the macro')
    
    args = parser.parse_args()
    
    success = update_latex_macros(args.file_path, args.macro_name, args.value, args.format)
    
    if success:
        print(f"Successfully updated macro \\{args.macro_name} in {args.file_path}")
    else:
        print(f"Failed to update macro \\{args.macro_name}")
        sys.exit(1)

if __name__ == "__main__":
    x = 437674730
    update_latex_macros("numbers.tex", "test", f'{x:,} milli', "number")