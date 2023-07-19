// Has a runtime error (segfault)
#include <vector>
#include <iostream>

using namespace std;

int abs(int a){
    if (a < 0) return -a;
    return a;
}

int min(int a, int b){
    return (a < b) ? a : b;
}

int max(int a, int b){
    return (a < b) ? b : a;
}

int main(){
  // Segfault immediately
  vector<int> arr;
  cout << arr[101];

  int N;
  cin >> N;
  for(int i = 0; i < N; i++){
      int l, n;
      vector<int> ants;

      cin >> l >> n;
      for (int j = 0; j < n; j++){
        int pos;
        cin >> pos;
        ants.push_back(pos);
      }

      // Shortest time
      int shortestT;
      {
          int longest = 0;
          for(int pos : ants){
            longest = max(longest, min(pos, l - pos));
          }
          shortestT = longest;
      }

      // Longest time
      int longestT;
      {
          int longest = 0;
          for(int pos : ants){
        longest = max(longest, max(pos, l - pos));
          }
          longestT = longest;
      }

      // Output
      cout << shortestT << " " << longestT << endl;
    }
}
