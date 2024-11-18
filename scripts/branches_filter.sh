#!/bin/bash

reject2021=`grep -E ";rejected" examples/branches_filter_2021.csv | wc -l`
keep2021=`grep -E ";keep" examples/branches_filter_2021.csv | wc -l`
total=$(echo "$reject2021 + $keep2021" | bc)
percentageReject=$(echo "scale=5; $reject2021 / ($reject2021 + $keep2021) * 100" | bc)
percentageKeep=$(echo "scale=5; $keep2021 / ($reject2021 + $keep2021) * 100" | bc)
printf "Percentage of branches rejected in Python 2021: %.2f%% -> %'d out of %'d\n" $percentageReject $reject2021 $total
printf "Percentage of branches kept in Python 2021: %.2f%% -> %'d out of %'d\n" $percentageKeep $keep2021 $total

rejectFULL=`grep -E ";rejected" examples/branches_filter_FULL.csv | wc -l`
keepFULL=`grep -E ";keep" examples/branches_filter_FULL.csv | wc -l`
total=$(echo "$rejectFULL + $keepFULL" | bc)
percentageReject=$(echo "scale=5; $rejectFULL / ($rejectFULL + $keepFULL) * 100" | bc)
percentageKeep=$(echo "scale=5; $keepFULL / ($rejectFULL + $keepFULL) * 100" | bc)
printf "Percentage of branches rejected in FULL 2023-09: %.2f%% -> %'d out of %d\n" $percentageReject $rejectFULL $total
printf "Percentage of branches kept in FULL 2023-09: %.2f%% -> %'d out of %'d\n" $percentageKeep $keepFULL $total