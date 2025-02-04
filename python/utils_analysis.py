import matplotlib.pyplot as plt
import numpy as np
from collections import Counter
import seaborn as sb

mask_couple = np.array([
    [True, True, True, True, False, False, False, False, False, False, True],
    [False, False, False, True, False, False, True, False, True, True, True],
    [False, False, True, False, False, True, False, True, False, True, True],
    [True, False, False, False, True, True, True, False, False, False, True],
    [False, True, False, False, True, False, False, True, True, False, True]
])

def add_relations_v1(alone, single, couple, triple, ll):
    for l in ll:
        l = sorted(l)
        if len(l) == 1:
            alone[l[0]] += 1
        for i in range(len(l)):
            single[l[i]] += 1
            for j in range(i+1, len(l)):
                couple.setdefault(l[i], Counter())
                couple[l[i]][l[j]] += 1
                for k in range(j+1, len(l)):
                    triple.setdefault(l[i], {}).setdefault(l[j], Counter())
                    triple[l[i]][l[j]][l[k]] += 1
                   
def add_relations_v2(alone, single, couple, triple, ll):
    for l in ll:
        l = sorted(l)
        if len(l) == 1:
            alone[l[0]] += 1
        for i in range(len(l)):
            single[l[i]] += 1
            for j in range(i+1, len(l)):
                couple[(l[i], l[j])] += 1
                for k in range(j+1, len(l)):
                    triple[(l[i], l[j], l[k])] += 1
    return len(ll)

def display_single(stats):
    plt.figure(figsize=(9, 6))
    colors = ['blue', 'blue', 'blue', 'blue', 'blue', 'orange', 'green', 'green', 'green', 'grey']
    categories = ['Author', 'Message', 'Date', 'Committer', 'CommitterDate', 'DifferentBranchName', 'FileModified', 'FileRemoved', 'ContentSplit', 'Other']
    values = []
    for i in range(len(categories)):
        values.append(stats[categories[i]])

    plt.bar(categories, values, color=colors)
    plt.grid(True, linestyle='--', alpha=0.3, axis='y')
    plt.ylabel('Number of Changes', fontsize=12)
    
    plt.xticks(range(len(categories)), categories, rotation=45, 
               horizontalalignment='right', fontsize=10)
    plt.yticks(fontsize=10)
    
    for i, value in enumerate(values):
        plt.text(i, value, f'{value:,}', ha='center', va='bottom')
        
    plt.margins(x=0.01)
    plt.tight_layout()
        
    plt.show()
    
def display_single_META(stats):
    colors = ['blue', 'blue', 'blue', 'blue', 'blue']
    categories = ['Author', 'Message', 'Date', 'Committer', 'CommitterDate']
    values = []
    for i in range(len(categories)):
        values.append(stats[categories[i]])
    plt.bar(categories, values, color=colors)
    plt.grid(True, linestyle='--', alpha=0.3, axis='y')
    plt.xticks(range(len(categories)), categories, ha='right')
    plt.xticks(rotation=45, horizontalalignment='right')
    
    for i, value in enumerate(values):
        plt.text(i, value, f'{value:,}', ha='center', va='bottom')
        
    #plt.tight_layout()
    plt.show()
    
def display_single_DIR(stats):
    colors = ['orange', 'green', 'green', 'green']
    categories = ['DifferentBranchName', 'FileModified', 'FileRemoved', 'ContentSplit']
    values = []
    for i in range(len(categories)):
        values.append(stats[categories[i]]) 
    categories[1] = 'FileModified'
    categories[2] = 'FileRemoved'
    categories[3] = 'ContentSplit'
    plt.bar(categories, values, color=colors)
    plt.grid(True, linestyle='--', alpha=0.3, axis='y')
    plt.xticks(range(len(categories)), categories, ha='right')
    plt.xticks(rotation=45, horizontalalignment='right')
    
    for i, value in enumerate(values):
        plt.text(i, value, f'{value:,}', ha='center', va='bottom')
        
    #plt.tight_layout()
    plt.show()
    
def stats_couple(couple, focus, alone, single, display=False):
    colors = ['blue', 'blue', 'blue', 'blue', 'purple']
    categories = {'Author': 0, 'Message': 0, 'Date': 0, 'Committer': 0, 'CommitterDate': 0}
    categories.pop(focus)
    for ((catA, catB), val) in couple.items():
        if catA == focus:
            categories.update({catB: val/single[focus]*100})
        if catB == focus:
            categories.update({catA: val/single[focus]*100})
    categories.update({'Alone': alone[focus]/single[focus]*100})
    if display:
        _, ax = plt.subplots()
        ax.bar(categories.keys(), categories.values(), color=colors)
        ax.set_title("Percentage of other categories when "+focus+" is changed")
        plt.xticks(range(len(categories)), categories, ha='right')
        plt.xticks(rotation=45, horizontalalignment='right')
        plt.tight_layout()
        plt.show()
    for (_,v) in categories.items():
        assert(categories['Alone'] + v <= 100)
    categories.update({focus: 100})
    return categories

def stats_couple_dir(couple, focus, alone, single, display=False):
    colors = ['blue', 'blue', 'blue', 'blue']
    categories = {'DifferentBranchName': 0, 'FileModified': 0, 'FileRemoved': 0, 'ContentSplit': 0} 
    categories.pop(focus)
    for ((catA, catB), val) in couple.items():
        if catA == focus:
            categories.update({catB: val/single[focus]*100})
        if catB == focus:
            categories.update({catA: val/single[focus]*100})
    categories.update({'Alone': alone[focus]/single[focus]*100})
    if display:
        _, ax = plt.subplots()
        ax.bar(categories.keys(), categories.values(), color=colors)
        ax.set_title("Percentage of other categories when "+focus+" is changed")
        plt.xticks(range(len(categories)), categories, ha='right')
        plt.xticks(rotation=45, horizontalalignment='right')
        plt.tight_layout()
        plt.show()
    for (categ,v) in categories.items():
        if categ == 'Alone':
            continue
        assert(categories['Alone'] + v <= 100)
    categories.update({focus: 100})
    return categories
    
def stats_triple(triple, focus, single, display=False):
    colors = ['blue', 'blue', 'blue', 'blue', 'blue', 'blue']
    categories = {}
    for ((catA, catB, catC), val) in triple.items():
        if catA == focus:
            categories.update({catB+" & "+catC: val/single[focus]*100})
        if catB == focus:
            categories.update({catA+" & "+catC: val/single[focus]*100})
        if catC == focus:
            categories.update({catA+" & "+catB: val/single[focus]*100})
    if display:
        _, ax = plt.subplots()
        ax.bar(categories.keys(), categories.values(), color=colors)
        ax.set_title("Percentage of other categories when "+focus+" is changed")
        plt.xticks(range(len(categories)), categories, ha='right')
        plt.xticks(rotation=45, horizontalalignment='right')
        plt.tight_layout()
        plt.show()
    return categories
        
def heatmap_couple(couple, alone, single, display=False):
    y = ['Author', 'Message', 'Date', 'Committer', 'CommitterDate']
    mat = np.ndarray((5,6), float, np.zeros((5,6)))
    for i in range(len(y)):
        stat = stats_couple(couple, y[i], alone, single)
        for j in range(len(y)):
            mat[i][j] = stat[y[j]]
        mat[i][-1] = stat['Alone']
    if display:
        x_labels=["Author", "Message", "Date", "Committer", "CommitterDate", "Alone"]
        y_labels=["Author", "Message", "Date", "Committer", "CommitterDate"]
        hm = sb.heatmap(mat, cmap="crest",annot=True, fmt=".1f", xticklabels=x_labels, yticklabels=y_labels, vmin=0, vmax=100)
        hm.set(xlabel="...this also changes", ylabel="When this changes...")
        #hm.xaxis.tick_top()
        #hm.set_title("Heatmap of when 2 changes occur at the same time in percentage")
        plt.xticks(rotation=45, horizontalalignment='right')
        plt.tight_layout()
        plt.show()
    return mat

def heatmap_couple_dir(couple, alone, single, display=False):
    y = ['DifferentBranchName', 'FileModified', 'FileRemoved', 'ContentSplit']
    mat = np.ndarray((4,5), float, np.zeros((4,5)))
    for i in range(len(y)):
        stat = stats_couple_dir(couple, y[i], alone, single)
        for j in range(len(y)):
            mat[i][j] = stat[y[j]]
        mat[i][-1] = stat['Alone']
    if display:
        x_labels=['DifferentBranchName', 'FileModified', 'FileRemoved', 'ContentSplit', "Alone"]
        y_labels=['DifferentBranchName', 'FileModified', 'FileRemoved', 'ContentSplit']
        hm = sb.heatmap(mat, cmap="crest",annot=True, fmt=".1f", xticklabels=x_labels, yticklabels=y_labels, vmin=0, vmax=100)
        hm.set(xlabel="...this also changes", ylabel="When this changes...")
        #hm.xaxis.tick_top()
        #hm.set_title("Heatmap of when 2 changes occur at the same time in percentage")
        plt.xticks(rotation=45, horizontalalignment='right')
        plt.tight_layout()
        plt.show()
    return mat

def heatmap_triple(triple, couple, single, alone, display=False):
    x = ['Author & Committer', 'Author & CommitterDate', 'Author & Date', 'Author & Message', 'Committer & CommitterDate', 'Committer & Date', 'Committer & Message', 'CommitterDate & Date', 'CommitterDate & Message', 'Date & Message', 'Alone']
    y = ['Author', 'Message', 'Date', 'Committer', 'CommitterDate']
    mat = np.ndarray((5,11), float, np.zeros((5,11)))
    #mask = np.ndarray((5,10), bool, np.array([[False]* 10]* 5))
    for i in range(len(y)):
        stat = stats_triple(triple, y[i], single)
        for j in range(len(x)):
            try:
                mat[i][j] = stat[x[j]]
            except:
                #mask[i][j] = True
                stat_couple = stats_couple(couple, y[i], alone, single, False)
                other_categ = x[j].split(' ')
                if len(other_categ) > 1:
                    other_categ.pop(1)
                for categ in other_categ:
                    if categ == y[i]:
                        continue
                    #print("maj pos ",i," ",j," -> categ ", categ, ": ", stat_couple[categ])
                    mat[i][j] = stat_couple[categ]
                    break
    if display:
        display_triple(mat)
    return mat

def display_triple(mat, mask=np.ndarray((0,0)), with_mask_couple=False):
    x = ['Author & Committer', 'Author & CommitterDate', 'Author & Date', 'Author & Message', 'Committer & CommitterDate', 'Committer & Date', 'Committer & Message', 'CommitterDate & Date', 'CommitterDate & Message', 'Date & Message', 'Alone']
    y = ['Author', 'Message', 'Date', 'Committer', 'CommitterDate']
    if mask.any():
        if with_mask_couple:
            mask = mask | mask_couple
        ht = sb.heatmap(mat, cmap="crest",annot=True, fmt=".0f", xticklabels=x, yticklabels=y, mask=mask, vmin=0, vmax=100)
    else:
        if with_mask_couple:
            ht = sb.heatmap(mat, cmap="crest",annot=True, fmt=".0f", xticklabels=x, yticklabels=y, mask=mask_couple, vmin=0, vmax=100)
        else:
            ht = sb.heatmap(mat, cmap="crest",annot=True, fmt=".0f", xticklabels=x, yticklabels=y, vmin=0, vmax=100)
    ht.set(xlabel="...this also changes", ylabel="When this changes...")
    #ht.xaxis.tick_top()
    #ht.set_title("Heatmap of when 2 or 3 changes occur at the same time in percentage")
    #plt.xticks(range(len(x)), x, ha='right')
    plt.xticks(rotation=45, horizontalalignment='right')
    #plt.tight_layout()
    plt.show()
    
    
def filter_percent(ht, percent_min, percent_max):
    mask = np.ndarray((5,11), bool, np.array([[False]* 11]* 5))
    for i in range(len(ht)):
        for j in range(len(ht[i])):
            if ht[i][j] < percent_min or ht[i][j] >= percent_max:
                mask[i][j] = True
    return mask
