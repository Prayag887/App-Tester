import 'package:app_tester_companion/features/proxy_safety/data/proxy_safety_repository.dart';
import 'package:app_tester_companion/features/proxy_safety/presentation/proxy_safety_view_model.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeRepository implements ProxySafetyRepository {
  ProxySafetyStatus current = const ProxySafetyStatus(
      isMonitoring: false, isVpnActive: false, caAvailable: false);
  int stopCalls = 0;
  int startCalls = 0;

  @override
  Future<ProxySafetyStatus> status() async => current;

  @override
  Future<ProxySafetyStatus> startVpn() async {
    startCalls += 1;
    return current;
  }

  @override
  Future<ProxySafetyStatus> installCa() async => current;

  @override
  Future<ProxySafetyStatus> removeCa() async => current;

  @override
  Future<ProxySafetyStatus> stopVpn() async {
    stopCalls += 1;
    return current = const ProxySafetyStatus(
      isMonitoring: false,
      isVpnActive: false,
      caAvailable: false,
      message: 'Direct networking resumed.',
    );
  }
}

void main() {
  test('loads Android USB capture status', () async {
    final repository = FakeRepository()
      ..current = const ProxySafetyStatus(
        isMonitoring: true,
        isVpnActive: true,
        caAvailable: true,
        host: '127.0.0.1',
        port: 8080,
        targetPackage: 'com.example.app',
      );
    final model = ProxySafetyViewModel(repository);

    await model.load();

    expect(model.status?.isVpnActive, isTrue);
    expect(model.status?.targetPackage, 'com.example.app');
    model.dispose();
  });

  test('stops VPN and restores direct networking', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.disconnect();

    expect(repository.stopCalls, 1);
    expect(model.status?.isVpnActive, isFalse);
    expect(model.status?.message, contains('Direct networking'));
    model.dispose();
  });

  test('connects configured capture to desktop', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.connect();

    expect(repository.startCalls, 1);
    model.dispose();
  });
}
