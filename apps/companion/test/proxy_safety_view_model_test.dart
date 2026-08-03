import 'package:app_tester_companion/features/proxy_safety/data/proxy_safety_repository.dart';
import 'package:app_tester_companion/features/proxy_safety/presentation/proxy_safety_view_model.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeRepository implements ProxySafetyRepository {
  ProxySafetyStatus current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> startMonitoring(
          {required String host, required int port}) async =>
      current = ProxySafetyStatus(
        isMonitoring: true,
        isVpnActive: false,
        host: host,
        port: port,
      );

  @override
  Future<ProxySafetyStatus> stopMonitoring() async => current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> startVpn(
          {required String host,
          required int port,
          required String targetPackage}) async =>
      current = ProxySafetyStatus(
          isMonitoring: false,
          isVpnActive: true,
          host: host,
          port: port,
          targetPackage: targetPackage);

  @override
  Future<ProxySafetyStatus> stopVpn() async => current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> status() async => current;
}

void main() {
  test('disconnects a desktop before VPN capture starts', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.disconnect();

    expect(model.status?.isMonitoring, isFalse);
    expect(model.status?.isVpnActive, isFalse);
  });

  test('starts the configured VPN from the companion controls', () async {
    final repository = FakeRepository()
      ..current = const ProxySafetyStatus(
        isMonitoring: true,
        isVpnActive: false,
        host: '127.0.0.1',
        port: 8080,
        targetPackage: 'dev.example.app',
      );
    final model = ProxySafetyViewModel(repository);
    await model.load();

    await model.connectVpn(
      host: '127.0.0.1',
      portText: '8080',
      targetPackage: 'dev.example.app',
    );

    expect(model.status?.isVpnActive, isTrue);
  });
}
