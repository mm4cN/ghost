#include "io.hpp"
#include <iostream>

void log_line(const std::string &msg) {
  std::cout << "[io] " << msg << std::endl;
}
