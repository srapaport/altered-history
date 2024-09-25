#!/bin/bash

# Define the input log file
input_log="./results/2021/new/2021-all.log"

# Define the output log files
log1="./results/2021/new/main.log"
log2="./results/2021/new/focus/focus.log"
log3="./results/2021/new/focus/classes/classes.log"

# Find the line numbers for the markers
main_complete_line=$(grep -n "Main work complete" "$input_log" | cut -d: -f1)
focus_complete_line=$(grep -n "Focus complete" "$input_log" | cut -d: -f1)

# Check if the markers were found
if [ -z "$main_complete_line" ] || [ -z "$focus_complete_line" ]; then
    echo "Markers not found in the log file."
    exit 1
fi

# Extract the log parts
sed -n "1,${main_complete_line}p" "$input_log" > "$log1"
sed -n "$((main_complete_line + 1)),${focus_complete_line}p" "$input_log" > "$log2"
sed -n "$((focus_complete_line + 1)),\$p" "$input_log" > "$log3"

echo "Logs have been separated into $log1, $log2, and $log3."