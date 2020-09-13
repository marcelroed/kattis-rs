#include <vector>
#include <unordered_map>
#include <iostream>
#include <cstring>

using namespace std;

struct Node{
  Node* parent = this;
  int rank = 0;
  int money = 0;
};

Node* find(Node* x){
  if (x->parent != x){
    x->parent = find(x->parent);
  }
  return x->parent;
}

void nodeUnion(Node* a, Node* b){
  Node* rootA = find(a), *rootB = find(b);

  // In same set
  if (rootA == rootB) return;
  // Not in same set
  if (rootA->rank < rootB->rank){
    Node* temp = rootA;
    rootA = rootB;
    rootB = temp;
  }
  // Merge
  rootB->parent = rootA;
  if(rootA->rank == rootB->rank)
    rootA->rank++;
}

bool query(Node* a, Node* b){
  return (find(a) == find(b));
}

int main(){
  int n, m;
  ios_base::sync_with_stdio(false);
  cin.tie(nullptr);
  cin >> n >> m;
  Node* nodes[n];
  for(int i = 0; i < n; i++){
    nodes[i] = new Node();
    cin >> nodes[i]->money;
  }
  int a, b;
  for(int i = 0; i < m; i++){
    cin >> a >> b;
    nodeUnion(nodes[a], nodes[b]);
  }

  unordered_map<Node*, int> sums;

  for(int i = 0; i < n; i++) {
      Node* parent = find(nodes[i]);
      if (sums.find(parent) == sums.end()) {
          sums[parent] = 0;
      }
      sums[parent] += nodes[i]->money;
  }

  for(auto el : sums) {
      if (el.second != 0) {
          cout << "IMPOSSIBLE";
          return 0;
      }
  }
  //alsdkfalfj compiler error!
  cout << "POSSIBLE";
}