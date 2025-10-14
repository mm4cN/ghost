#include <string>
extern "C" {
#include "add.h"
}
#include "io/io.hpp"

int main() {
  log_line("Ghost in the Shell initializing...");
  int r = add(7, 5);
  log_line("Computation result: " + std::to_string(r));
  log_line("System integrity: stable.");
  return 0;
}
