cmake_minimum_required(VERSION 3.8)
project(mock_pkg)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)
find_package(sensor_msgs REQUIRED)

# This is a mock package — no actual source to build.
# It exists to test that rosup can resolve its Autoware deps.

ament_package()
