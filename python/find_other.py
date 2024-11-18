import os
import json
import requests
import pandas as pd
import numpy as np
import re
import time

#print("I'm waiting 1 hour before starting")
#time.sleep(3000)

token = "eyJhbGciOiJIUzI1NiIsInR5cCIgOiAiSldUIiwia2lkIiA6ICJhMTMxYTQ1My1hM2IyLTQwMTUtODQ2Ny05MzAyZjk3MTFkOGEifQ.eyJpYXQiOjE3MjI5NDUzMjcsImp0aSI6IjhmNGZjNzllLTU3OGItNGMwNi04NThkLWFmOTk3NTllMWM5ZCIsImlzcyI6Imh0dHBzOi8vYXV0aC5zb2Z0d2FyZWhlcml0YWdlLm9yZy9hdXRoL3JlYWxtcy9Tb2Z0d2FyZUhlcml0YWdlIiwiYXVkIjoiaHR0cHM6Ly9hdXRoLnNvZnR3YXJlaGVyaXRhZ2Uub3JnL2F1dGgvcmVhbG1zL1NvZnR3YXJlSGVyaXRhZ2UiLCJzdWIiOiIzNGUyMTU5Yi1lOGEyLTQ5ODItODZiYy1kM2VhYzU5Y2EyYzAiLCJ0eXAiOiJPZmZsaW5lIiwiYXpwIjoic3doLXdlYiIsInNlc3Npb25fc3RhdGUiOiJkMjIyZmNjNi1hNzY0LTQ5N2UtODUzZi1kYzNmOWM3MTU5NjQiLCJzY29wZSI6Im9wZW5pZCBvZmZsaW5lX2FjY2VzcyBwcm9maWxlIGVtYWlsIn0.dNJajd3CfFUzSpuy8PA2RrHbES4xzjlTuEV-SZxppKo"
headers = {
    "Authorization": f"Bearer {token}"
}
prefix = "https://archive.softwareheritage.org/api/1/revision/"
prefix_file = "/home/infres/rapaport/results/FULL/focus/classes"
pattern = r".*swh:1:rev:([a-f0-9]{40})"
class_files = os.listdir(prefix_file)
df = pd.DataFrame()
for file in class_files:
    df = pd.concat([df, pd.read_csv(prefix_file + "/" + file, delimiter=";")])
df['sub_categ_clean'] = list(map(lambda categ: categ.strip("{}").split(", "), df["sub_categories"]))

others = df[df['sub_categ_clean'].apply(lambda x: '' in x)]

start_time = time.time()
diff = {}
nb_request = 0
for mc in others['missing_commit']:
    nb_request += 1
    filter = others[others['missing_commit'] == mc]
    for fd in filter['first_difference']:
        match_mc = re.search(pattern, mc)
        if match_mc:
            extracted_hash_mc = match_mc.group(1)
            #print(extracted_hash_mc)
        else:
            print("No match found")
        match_fd = re.search(pattern, fd)
        if match_fd:
            extracted_hash_fd = match_fd.group(1)
            #print(extracted_hash_fd)
        else:
            print("No match found")
        resp_mc = requests.get(prefix + extracted_hash_mc, headers=headers).json()
        resp_fd = requests.get(prefix + extracted_hash_fd, headers=headers).json()
        for field in resp_mc:
            if field == 'url' or field == 'history_url' or field == 'id':
                continue
            if resp_mc[field] != resp_fd[field]:
                diff.setdefault(field, [])
                try:
                    diff[field].append(((resp_mc['id'], resp_mc[field]), (resp_fd['id'], resp_fd[field])))
                except:
                    print("error: ")
                    print(resp_mc)
                    print(resp_fd)
                #print("mc id: ", resp_mc['id'], "\nheader: ",resp_mc[field], "\nfd id: ", resp_fd['id'], "\nheader: ", resp_fd[field])
        time.sleep(4)
        if nb_request % 200 == 0:
            #print("fields:")
            #for i in diff:
                #print("\t", i)
            with open("others.tmp.json", "w") as outfile:
                json.dump(diff, outfile, indent=4)

            #print("--- %s seconds ---" % (time.time() - start_time))
            #exit(0)

print("fields:")
for i in diff:
    print("\t", i)
with open("others.json", "w") as outfile:
    json.dump(diff, outfile, indent=4)

print("--- %s seconds ---" % (time.time() - start_time))